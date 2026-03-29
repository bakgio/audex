// Write tags to an audio file using audex-wasm.
//
// Usage: node write_tags.mjs <audio_file>
//
// Modifies the file in-place. Demonstrates setting text tags, cover art,
// per-tag encoding, and format-specific save options.

import { readFileSync, writeFileSync } from "node:fs";
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
  console.error("Usage: node write_tags.mjs <audio_file>");
  process.exit(1);
}

const filePath = resolve(args[0]);
const filename = basename(filePath);

// Load the file
const bytes = new Uint8Array(readFileSync(filePath));
const file = new AudioFile(bytes, filename);
const fmt = file.formatName();

console.log(`Format: ${fmt}`);
console.log(`File:   ${filePath} (${bytes.length} bytes)`);

// Ensure the file has a tag container (required for untagged files)
if (!file.hasTags()) {
  file.addTags();
  console.log("  Created empty tag container");
}

// ── Set tags ────────────────────────────────────────────────────────────
// file.set(key, values) uses format-appropriate tag keys automatically.
// For ID3 (MP3, AIFF, WAV): use frame IDs like "TIT2", "TPE1", "TXXX:Key"
// For Vorbis (FLAC, OGG):   use lowercase keys like "title", "artist"
// For MP4:                   use atom codes like "©nam", "©ART"
// For APE:                   use title-case keys like "Title", "Artist"

file.set("TIT2", ["My Song Title"]);
file.set("TPE1", ["Artist Name"]);
file.set("TALB", ["Album Name"]);
file.set("TRCK", ["1/12"]);
file.set("TDRC", ["2024"]);
file.set("TCON", ["Rock"]);

console.log("  Tags written");

// ── Per-tag encoding (ID3 only) ─────────────────────────────────────────
// setTagWithEncoding(key, values, encoding) lets you control text encoding:
//   0 = Latin-1, 1 = UTF-16 (with BOM), 2 = UTF-16BE, 3 = UTF-8
//
// Example: write a TXXX frame with UTF-8 encoding
if (file.setTagWithEncoding) {
  file.setTagWithEncoding("TXXX:My Custom Tag", ["custom value"], 3);
  console.log("  Custom TXXX tag written with UTF-8 encoding");
}

// ── Cover art ───────────────────────────────────────────────────────────
// Different formats use different cover art methods:
//
//   ID3 (MP3, AIFF, WAV, DSF, DSDIFF):
//     file.setCoverArt(imageBytes, "image/jpeg")
//
//   FLAC:
//     file.setFlacCoverArt(imageBytes, "image/jpeg", pictureType, w, h, depth)
//
//   Vorbis (OGG Vorbis, OGG Opus, OGG Speex, OGG Theora, OGG FLAC):
//     file.setVorbisCoverArt(imageBytes, "image/jpeg", pictureType, w, h, depth)
//
//   MP4:
//     file.setMp4CoverArt(imageBytes, "image/jpeg")
//
//   APE (MonkeysAudio, Musepack, WavPack, TrueAudio, OptimFROG, TAK):
//     file.setApeCoverArt(imageBytes)
//
//   ASF/WMA:
//     file.setAsfCoverArt(imageBytes)

// ── Save ────────────────────────────────────────────────────────────────
// file.save() returns the complete file as a Uint8Array with updated tags.
// For MP3, saveWithOptions gives control over ID3 version and v1 tags:
//   file.saveWithOptions(v1, v2_version, v23_sep, convert_v24_frames)
//     v1: 0=REMOVE, 1=UPDATE, 2=CREATE
//     v2_version: 3 or 4
//     v23_sep: multi-value separator for v2.3 (e.g. "/")
//     convert_v24_frames: convert v2.4 frames to v2.3 equivalents

const savedBytes = file.save();
file.free();

writeFileSync(filePath, savedBytes);
console.log(`Saved:  ${filePath} (${savedBytes.length} bytes)`);
