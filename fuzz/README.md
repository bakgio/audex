# Fuzzing audex

Coverage-guided fuzz testing for every audio format parser in audex, covering
both sync and async code paths.

## Prerequisites

`cargo-fuzz` uses libFuzzer (LLVM) and requires:

- **Linux or macOS** (or WSL on Windows)
- **Nightly Rust** (`rustup install nightly`)
- A C++ compiler with sanitizer support

```bash
cargo install cargo-fuzz
rustup install nightly
```

> **Windows users:** Run all fuzzing commands inside WSL. The audex source tree
> can be accessed from WSL via `/mnt/c/Users/...` or cloned directly inside WSL
> for better I/O performance.

## Quick start

```bash
cd audex

# List all available targets
cargo +nightly fuzz list

# Run a single target
cargo +nightly fuzz run fuzz_flac

# Run with recommended limits
cargo +nightly fuzz run fuzz_flac -- \
    -rss_limit_mb=512 \
    -max_len=1048576 \
    -timeout=10
```

## Targets

Every format has both a **sync** and **async** fuzz target. Async targets
exercise the `_async` code paths using a single-threaded tokio runtime.

| Format         | Sync target           | Async target                |
|----------------|-----------------------|-----------------------------|
| Auto-detect    | `fuzz_auto_detect`    | `fuzz_async_auto_detect`    |
| Auto-detect (ext) | `fuzz_auto_detect_ext` | `fuzz_async_auto_detect_ext` |
| FLAC           | `fuzz_flac`           | `fuzz_async_flac`           |
| MP4/M4A        | `fuzz_mp4`            | `fuzz_async_mp4`            |
| ID3v2          | `fuzz_id3`            | `fuzz_async_id3`            |
| ID3 structured | `fuzz_id3_structured` | —                           |
| OGG container  | `fuzz_ogg`            | `fuzz_async_ogg`            |
| ASF/WMA        | `fuzz_asf`            | `fuzz_async_asf`            |
| MP3            | `fuzz_mp3`            | `fuzz_async_mp3`            |
| APEv2          | `fuzz_apev2`          | `fuzz_async_apev2`          |
| AAC            | `fuzz_aac`            | `fuzz_async_aac`            |
| AC3/E-AC3      | `fuzz_ac3`            | `fuzz_async_ac3`            |
| AIFF           | `fuzz_aiff`           | `fuzz_async_aiff`           |
| WAV            | `fuzz_wave`           | `fuzz_async_wave`           |
| DSF            | `fuzz_dsf`            | `fuzz_async_dsf`            |
| DSDIFF         | `fuzz_dsdiff`         | `fuzz_async_dsdiff`         |
| Musepack       | `fuzz_musepack`       | `fuzz_async_musepack`       |
| WavPack        | `fuzz_wavpack`        | `fuzz_async_wavpack`        |
| Monkey's Audio | `fuzz_monkeysaudio`   | `fuzz_async_monkeysaudio`   |
| TAK            | `fuzz_tak`            | `fuzz_async_tak`            |
| TrueAudio      | `fuzz_trueaudio`      | `fuzz_async_trueaudio`      |
| OptimFROG      | `fuzz_optimfrog`      | `fuzz_async_optimfrog`      |
| SMF/MIDI       | `fuzz_smf`            | `fuzz_async_smf`            |
| OGG Vorbis     | `fuzz_oggvorbis`      | `fuzz_async_oggvorbis`      |
| OGG FLAC       | `fuzz_oggflac`        | `fuzz_async_oggflac`        |
| OGG Opus       | `fuzz_oggopus`        | `fuzz_async_oggopus`        |
| OGG Speex      | `fuzz_oggspeex`       | `fuzz_async_oggspeex`       |
| OGG Theora     | `fuzz_oggtheora`      | `fuzz_async_oggtheora`      |
| EasyID3        | `fuzz_easyid3`        | `fuzz_async_easyid3`        |
| EasyMP3        | `fuzz_easymp3`        | `fuzz_async_easymp3`        |
| EasyMP4        | `fuzz_easymp4`        | `fuzz_async_easymp4`        |

## Recommended libFuzzer flags

| Flag             | Value     | Purpose                                       |
|------------------|-----------|-----------------------------------------------|
| `-rss_limit_mb`  | `512`     | Detect OOM bugs (biggest vulnerability class)  |
| `-max_len`       | `1048576` | Cap input size at 1 MiB                        |
| `-timeout`       | `10`      | Detect infinite loops / algorithmic complexity  |
| `-jobs`          | `N`       | Run N parallel fuzzing jobs                    |
| `-workers`       | `N`       | Use N worker processes                         |
| `-max_total_time`| `3600`    | Time-boxed run (seconds) for CI                |

## Running all targets in parallel

```bash
# Run every sync target for 30 minutes each, 4 jobs per target
for target in $(cargo +nightly fuzz list); do
    cargo +nightly fuzz run "$target" -- \
        -max_total_time=1800 \
        -rss_limit_mb=512 \
        -max_len=1048576 \
        -timeout=10 \
        -jobs=4 \
        -workers=4 &
done
wait
```

## Reproducing crashes

When the fuzzer finds a crash, it saves the input to `fuzz/artifacts/<target>/`.

```bash
# Reproduce a crash
cargo +nightly fuzz run fuzz_flac fuzz/artifacts/fuzz_flac/crash-abc123

# Minimize the crashing input
cargo +nightly fuzz tmin fuzz_flac fuzz/artifacts/fuzz_flac/crash-abc123

# Minimize the corpus (remove redundant inputs)
cargo +nightly fuzz cmin fuzz_flac

# Generate coverage report
cargo +nightly fuzz coverage fuzz_flac
```

## Architecture

Most fuzz targets follow this pattern:

1. **Size check** — reject inputs larger than 2 MiB to avoid fuzzer-side OOM
2. **Write to temp file** — format parsers require filesystem paths
3. **Load** — call the format-specific `load()` (sync) or `load_async()` (async)
4. **Exercise API** — call lightweight wrapper methods such as `info()`,
   `keys()`, `items()`, `to_tag_map()`, and, when the loaded representation
   supports it, save/clear paths against either a temp file or in-memory writer

The auto-detect targets are intentionally broader than the format-specific ones:
they focus on dynamic dispatch and wrapper behavior after load rather than on
format-specific tag mutation semantics.

Async targets create a single-threaded tokio runtime per invocation and
`block_on` the async loading path, then exercise the async wrapper methods that
are valid for the successfully loaded file.

The `fuzz_auto_detect` target uses `File::load_from_reader()` (sync) or
`File::load_from_buffer_async()` (async), which exercises the format scoring
system and dispatches into every parser — making it the single highest-value
target.

The `fuzz_auto_detect_ext` target complements `fuzz_auto_detect` by writing
fuzz data to a temp file with a random supported extension (mp3, flac, m4a,
etc.) and loading via `File::load()`. This exercises the extension-based
format scoring heuristics that the reader-based target bypasses.

The `fuzz_id3_structured` target uses `arbitrary` to generate syntactically
plausible ID3v2 headers and frames, giving the fuzzer a head start at reaching
deep frame parsing logic.

## What the fuzzer finds

Expect crashes in these categories:

- **OOM** — `vec![0u8; untrusted_size as usize]` allocations from format headers
- **Integer overflow** — unchecked `pos + length` arithmetic wrapping past bounds
- **Infinite loops** — metadata block loops without termination flags
- **Panics** — unchecked slice indexing, unwrap on parsing results
- **Stack overflow** — recursive atom parsing (MP4) with deeply nested containers
