// Extract all tags and stream info from an audio file to JSON.
//
// Usage: node extract_tags_json.mjs <audio_file> [output.json]
//
// If no output path is given, prints to stdout.

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
  console.error("Usage: node extract_tags_json.mjs <audio_file> [output.json]");
  process.exit(1);
}

const inputPath = resolve(args[0]);
const outputPath = args[1] ? resolve(args[1]) : null;
const filename = basename(inputPath);

// Load the file
const bytes = new Uint8Array(readFileSync(inputPath));
const file = new AudioFile(bytes, filename);

// Format
const format = file.formatName();

// Stream info
const si = file.streamInfo();
const info = {
  length: si.lengthSecs ?? null,
  bitrate: si.bitrate ?? null,
  sample_rate: si.sampleRate ?? null,
  channels: si.channels ?? null,
  bits_per_sample: si.bitsPerSample ?? null,
};
si.free();

// Tags
const rawTags = file.tagsJson();
const tags = {};
let tagCount = 0;

// Vorbis-based formats store values as arrays; others as single strings
const ARRAY_FORMATS = [
  "FLAC", "OggVorbis", "OggOpus", "OggFlac", "OggSpeex", "OggTheora",
  "ASF", "MP4",
];
const useArrays = ARRAY_FORMATS.includes(format);

if (rawTags && typeof rawTags === "object") {
  for (const [key, values] of Object.entries(rawTags)) {
    if (useArrays) {
      tags[key] = Array.isArray(values) ? values : [String(values)];
    } else {
      if (Array.isArray(values) && values.length === 1) {
        tags[key] = values[0];
      } else if (Array.isArray(values)) {
        tags[key] = values.join("\0");
      } else {
        tags[key] = String(values);
      }
    }
    tagCount++;
  }
}

// APE binary tags (cover art) — produce a size+fingerprint summary
const APE_BINARY_KEYS = ["Cover Art (Front)", "Cover Art (Back)", "Cover Art (Icon)"];
for (const binKey of APE_BINARY_KEYS) {
  if (tags[binKey]) continue;
  try {
    const data = file.getApeBinaryTag(binKey);
    if (data && data.length > 0) {
      tags[binKey] = `<binary:${data.length} bytes>`;
      tagCount++;
    }
  } catch (_) {}
}

// MP4 cover art
try {
  const coverData = file.getMp4CoverArt();
  if (coverData && coverData.length > 0) {
    const val = `<binary:${coverData.length} bytes>`;
    tags["covr"] = useArrays ? [val] : val;
    tagCount++;
  }
} catch (_) {}

// Build result
const result = { file: inputPath, format, info, tags, tag_count: tagCount };
const json = JSON.stringify(result, null, 2);

if (outputPath) {
  writeFileSync(outputPath, json);
  console.error(`Extracted ${tagCount} tags from ${filename} (${format})`);
  console.error(`Saved to: ${outputPath}`);
} else {
  console.log(json);
}

file.free();
