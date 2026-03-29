// Demonstrates the tag-diffing API via audex-wasm.
//
// Usage: node diff_tags.mjs <file_a> <file_b>
//
// Mirrors audex/examples/diff_tags.rs — compares the metadata of two audio
// files and prints the results using every available diff method.

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
if (args.length < 2) {
  console.error("Usage: node diff_tags.mjs <file_a> <file_b>");
  process.exit(1);
}

const pathA = resolve(args[0]);
const pathB = resolve(args[1]);

// Load both files
const fileA = new AudioFile(new Uint8Array(readFileSync(pathA)), basename(pathA));
const fileB = new AudioFile(new Uint8Array(readFileSync(pathB)), basename(pathB));

console.log(`File A: ${pathA} (${fileA.formatName()})`);
console.log(`File B: ${pathB} (${fileB.formatName()})`);

// --- Basic diff ---
// diffTags() compares format-specific keys as-is (e.g. TIT2 vs title).
console.log("\n=== Basic diff ===");
const d = fileA.diffTags(fileB);
if (d.isIdentical()) {
  console.log("Tags are identical.");
} else {
  console.log(d.pprint());
}

// --- Summary ---
console.log("\n=== Summary ===");
console.log(d.summary());

// --- Pretty-print (right-aligned keys) ---
console.log("\n=== Pretty-print ===");
console.log(d.pprint());

// --- Diff with options (stream info + case-insensitive) ---
console.log("\n=== With options (stream info + case-insensitive) ===");
const d2 = fileA.diffTagsWithOptions(
  fileB,
  true,   // compare_stream_info
  true,   // case_insensitive_keys
  null,   // trim_values
  true,   // include_unchanged
);
console.log(d2.pprint());

// --- Full pretty-print (including unchanged) ---
console.log("\n=== Full pretty-print ===");
console.log(d2.pprintFull());

// --- Snapshot-based diffing ---
console.log("\n=== Snapshot-based diff ===");
const snapshot = fileA.snapshotTags();
const d3 = fileB.diffAgainstSnapshot(snapshot);
console.log(d3.summary());

// --- Filtering ---
console.log("\n=== Filtered diff (artist only) ===");
const filtered = d.filterKeys(["artist", "ARTIST", "TPE1", "\u00a9ART"]);
if (filtered.diffCount() === 0) {
  console.log("No artist differences.");
} else {
  console.log(filtered.pprint());
}

// --- Normalised diff ---
// diffTagsNormalized() maps keys through TagMap so that format-specific
// names become their standard equivalents (e.g. TIT2 → Title).
console.log("\n=== Normalised diff ===");
const d4 = fileA.diffTagsNormalized(fileB);
console.log(d4.summary());

// --- Normalised diff with stream info ---
console.log("\n=== Normalised diff (with stream info) ===");
const d5 = fileA.diffTagsNormalized(fileB, true, true);
console.log(d5.pprintFull());

// --- toJson() for raw data access ---
console.log("\n=== toJson() ===");
const json = d4.toJson();
console.log(`  changed: ${json.changed.length}, left_only: ${json.left_only.length}, right_only: ${json.right_only.length}, unchanged: ${json.unchanged.length}`);

// --- differingKeys() ---
console.log("\n=== Differing keys ===");
const keys = d4.differingKeys();
console.log(`  ${keys.length} keys differ: ${keys.slice(0, 10).join(", ")}${keys.length > 10 ? "..." : ""}`);

// Free all WASM objects
filtered.free();
d.free();
d2.free();
d3.free();
d4.free();
d5.free();
fileA.free();
fileB.free();
