# Contributing to Audex

Thanks for your interest in contributing to Audex! This document covers everything you need to get started.

## Getting Started

### Prerequisites

- **Rust 1.85+** (the minimum supported Rust version)
- **Git**

Optional, depending on what you're working on:

- **Nightly Rust** — required for fuzzing (`rustup install nightly`)
- **wasm-pack 0.13.1** — required for WASM builds
- **Node.js** — required for running WASM examples

### Building

```bash
git clone https://github.com/bakgio/audex.git
cd audex

# Build with default features
cargo build

# Build with all features
cargo build --all-features
```

### Running Tests

```bash
# Run the full test suite (recommended)
cargo test --all-features

# Run a specific test
cargo test --all-features test_flac_read_write

# Run WASM tests (requires wasm-pack and Node.js)
cd wasm && wasm-pack test --node
```

### Linting and Formatting

CI enforces both — run these before submitting a PR:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Making Changes

### Bug Fixes

1. Open an issue describing the bug (if one doesn't exist already).
2. Include a minimal reproducing case — ideally a test file or byte sequence.
3. Write a regression test that fails without the fix and passes with it.
4. Submit a PR referencing the issue.

### New Format Support

If you're adding support for a new audio format:

1. Add the parser/writer in `src/` following the existing format modules as a guide.
2. Add integration tests in `tests/` with sample audio files in `tests/data/`.
3. Add both sync and async fuzz targets in `fuzz/fuzz_targets/`.
4. Update the format table in `README.md`.

### New Features

For larger changes, open an issue first to discuss the design. This saves everyone time if the approach needs adjustment.

## Code Guidelines

- Follow existing code style — `cargo fmt` handles formatting.
- All public API items should have doc comments.
- Keep `unsafe` usage to an absolute minimum. If you must use it, explain why in a comment.
- Parsers must handle malformed input gracefully — return errors, don't panic. The fuzz suite will catch these.

## Testing

### Test Fixtures

Test audio files live in `tests/data/`. When adding new ones:

- Keep files as small as possible (a few KB is ideal).
- Include only the minimum metadata needed for the test.
- Do not commit copyrighted audio content.

### Fuzzing

If your change touches any parser or writer code, run the relevant fuzz target:

```bash
cargo +nightly fuzz run fuzz_<format> -- \
    -rss_limit_mb=512 \
    -max_len=1048576 \
    -timeout=10 \
    -max_total_time=300
```

See [`fuzz/README.md`](fuzz/README.md) for the full list of targets and recommended flags.

> **Note:** Fuzzing requires Linux or macOS (or WSL on Windows).

## WASM

If your change affects the public API, check that the WASM bindings still compile and pass tests:

```bash
cd wasm

# Compile check
cargo check --target wasm32-unknown-unknown

# Run WASM integration tests (requires wasm-pack 0.13.1 and Node.js)
wasm-pack test --node
```

## Submitting a Pull Request

1. Fork the repo and create a branch from `main`.
2. Make your changes — keep commits focused and well-described.
3. Ensure `cargo test --all-features`, `cargo fmt`, and `cargo clippy` all pass.
4. Open a PR against `main` with a clear description of what changed and why.

CI runs automatically on all PRs. All checks must pass before merge.

## License

By contributing to Audex, you agree that your contributions will be dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE), as described in the [README](README.md#license).
