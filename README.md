<p align="center">
  <h1 align="center">Audex</h1>
  <p align="center">
    Audio metadata reading and writing for Rust — one API, 20+ formats.
  </p>
  <p align="center">
    <a href="https://crates.io/crates/audex"><img src="https://img.shields.io/crates/v/audex.svg" alt="Crates.io"></a>
    &nbsp;&nbsp;
    <a href="https://docs.rs/audex"><img src="https://img.shields.io/docsrs/audex" alt="docs.rs"></a>
    &nbsp;&nbsp;
    <a href="LICENSE-MIT"><img src="https://img.shields.io/crates/l/audex.svg" alt="License"></a>
    &nbsp;&nbsp;
    <img src="https://img.shields.io/badge/MSRV-1.85-blue.svg" alt="MSRV 1.85">
  </p>
</p>

---

- **Unified API** — dictionary-style `get`/`set`/`remove` across all writable formats
- **Automatic format detection** — magic bytes, extension, and content analysis
- **Easy wrappers** — `EasyID3` and `EasyMP4` for human-readable key names (`"artist"` instead of `"TPE1"`)
- **Async support** — optional Tokio-based async I/O (`async` feature flag)
- **Serialization** — optional serde support for JSON, TOML, and more (`serde` feature flag)
- **Cross-format tag conversion** — transfer metadata between any two formats with `convert_tags()` (optional `serde` feature for serializable reports)
- **Tag diffing** — compare metadata between files or snapshots with `diff_tags()`, with support for normalized cross-format comparison, filtering, and pretty-printing (optional `serde` feature for serializable diff results)
- **Structured logging** — optional tracing integration for observability (`tracing` feature flag)
- **Flexible I/O** — load from file paths, in-memory buffers, or any `Read + Seek` source
- **Batch tag updates** — update multiple tags at once with `file.update()`
- **Stream info** — sample rate, bitrate, channels, duration, bits per sample

## Installation

```toml
[dependencies]
audex = "0.2.0"

# With optional features:
# audex = { version = "0.2.0", features = ["async"] }
# audex = { version = "0.2.0", features = ["serde"] }
# audex = { version = "0.2.0", features = ["tracing"] }
```

## Feature Flags

| Feature | Description |
|:---|:---|
| `async` | Tokio-based async file I/O (`load_async`, `save_async`) |
| `serde` | Serialize/deserialize all public types to JSON, TOML, and any serde-supported format |
| `tracing` | Structured logging via the `tracing` crate for observability and debugging |

All features are opt-in and zero-cost when disabled.

> See the [`examples/`](examples/) directory for reading tags, writing tags, and file operations.

## Supported Formats

| Format | Tag System | Tags | Stream Info |
|:---|:---|:---:|:---:|
| MP3 | ID3v1, ID3v2 | R/W | Yes |
| FLAC | Vorbis Comments | R/W | Yes |
| MP4 / M4A / M4B | iTunes atoms | R/W | Yes |
| Ogg Vorbis | Vorbis Comments | R/W | Yes |
| Ogg Opus | Vorbis Comments | R/W | Yes |
| Ogg Speex | Vorbis Comments | R/W | Yes |
| Ogg FLAC | Vorbis Comments | R/W | Yes |
| Ogg Theora | Vorbis Comments | R/W | Yes |
| WAV | ID3v2 | R/W | Yes |
| AIFF | ID3v2 | R/W | Yes |
| WavPack | APEv2 | R/W | Yes |
| Monkey's Audio | APEv2 | R/W | Yes |
| Musepack | APEv2 | R/W | Yes |
| OptimFROG | APEv2 | R/W | Yes |
| TAK | APEv2 | R/W | Yes |
| TrueAudio | ID3v1/v2, APEv2 | R/W | Yes |
| ASF / WMA | ASF attributes | R/W | Yes |
| DSF (DSD) | ID3v2 | R/W | Yes |
| DSDIFF (DFF) | ID3v2 | R/W | Yes |
| AAC (ADTS/ADIF) | — | — | Yes |
| AC-3 / E-AC-3 | — | — | Yes |
| SMF / MIDI | — | — | Duration |

## WebAssembly

Audex ships with WASM bindings in the [`wasm/`](wasm/) crate, bringing full tag reading, writing, and stream info to browsers and Node.js — no filesystem access required.

See the [`wasm/examples/`](wasm/examples/) directory for reading, writing, and extracting tags via Node.js.

Pre-built packages for web and Node.js are attached to each [GitHub Release](https://github.com/bakgio/audex/releases). To build from source:

```bash
cd wasm
make build-web   # browser build  -> pkg/
make build-node  # Node.js build  -> pkg-node/
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Audex by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
