// Read and display tags from an audio file using audex-wasm.
//
// Usage: node read_tags.mjs <audio_file>
//
// Supports all formats audex handles: MP3, FLAC, OGG, MP4, WAV, AIFF,
// APE, WavPack, Musepack, DSF, DSDIFF, ASF/WMA, and more.

import { readFileSync } from "node:fs";
import { resolve, basename, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_DIR = resolve(__dirname, "../pkg");

// Load and initialise the WASM module
const wasmBytes = readFileSync(resolve(PKG_DIR, "audex_wasm_bg.wasm"));
const { initSync, AudioFile } = await import(
  pathToFileURL(resolve(PKG_DIR, "audex_wasm.js")).href
);
initSync({ module: wasmBytes });

// ── Main ────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
if (args.length < 1) {
  console.error("Usage: node read_tags.mjs <audio_file>");
  process.exit(1);
}

const inputPath = resolve(args[0]);
const filename = basename(inputPath);

// Load the file
const bytes = new Uint8Array(readFileSync(inputPath));
const file = new AudioFile(bytes, filename);

// Format detection
console.log(`Format: ${file.formatName()}`);
console.log(`File:   ${inputPath} (${bytes.length} bytes)`);

// Stream info
const si = file.streamInfo();
console.log("\n── Stream Info ──");
if (si.lengthSecs != null) console.log(`  Duration:    ${si.lengthSecs.toFixed(2)}s`);
if (si.bitrate != null) console.log(`  Bitrate:     ${si.bitrate} bps`);
if (si.sampleRate != null) console.log(`  Sample rate: ${si.sampleRate} Hz`);
if (si.channels != null) console.log(`  Channels:    ${si.channels}`);
if (si.bitsPerSample != null) console.log(`  Bit depth:   ${si.bitsPerSample}`);
si.free();

// Tags
const tags = file.tagsJson();
if (tags && Object.keys(tags).length > 0) {
  console.log("\n── Tags ──");
  const keys = Object.keys(tags).sort();
  for (const key of keys) {
    const val = tags[key];
    const display = Array.isArray(val) ? val.join("; ") : String(val);
    // Truncate long values for display
    const truncated = display.length > 120 ? display.slice(0, 117) + "..." : display;
    console.log(`  ${key}: ${truncated}`);
  }
  console.log(`\n  Total: ${keys.length} tags`);
} else {
  console.log("\n  No tags found.");
}

file.free();
