// Convert tags from one audio file to another using audex-wasm.
//
// Usage: node convert_tags.mjs <source> <destination>
//
// Demonstrates cross-format tag conversion: loads the tagged source file,
// converts its tags into the destination file via importTagsFrom() (which
// maps fields through TagMap), and saves the result.

import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
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
  console.error("Usage: node convert_tags.mjs <source> <destination>");
  process.exit(1);
}

const sourcePath = resolve(args[0]);
const destPath = resolve(args[1]);
const tempDestPath = `${destPath}.audex.tmp`;

if (sourcePath === destPath) {
  throw new Error("source and destination must be different files");
}

// Load both files
const source = new AudioFile(new Uint8Array(readFileSync(sourcePath)), basename(sourcePath));
const dest = new AudioFile(new Uint8Array(readFileSync(destPath)), basename(destPath));

console.log(`Source:      ${sourcePath} (${source.formatName()})`);
console.log(`Destination: ${destPath} (${dest.formatName()})`);
console.log("Writing changes through a temporary file before replacing the destination.");

// ── Convert tags ────────────────────────────────────────────────────────
// importTagsFrom() extracts the source's TagMap (standard + custom fields)
// and writes them into the destination file, mapping field names between
// formats automatically (e.g. ID3 TIT2 → Vorbis title → MP4 ©nam).

const report = dest.importTagsFrom(source);

console.log("\n── Conversion Report ──");

// Standard fields transferred
const transferred = report.transferred || [];
console.log(`Transferred: ${transferred.length} standard fields`);
for (const field of transferred) {
  console.log(`  + ${field}`);
}

// Custom fields transferred
const custom = report.custom_transferred || [];
if (custom.length > 0) {
  console.log(`Custom fields: ${custom.length}`);
  for (const key of custom) {
    console.log(`  + ${key}`);
  }
}

// Skipped fields
const skipped = report.skipped || [];
if (skipped.length > 0) {
  console.log(`Skipped: ${skipped.length}`);
  for (const entry of skipped) {
    if (Array.isArray(entry)) {
      console.log(`  - ${entry[0]} (${entry[1]})`);
    } else {
      console.log(`  - ${entry.field || entry} (${entry.reason || "unknown"})`);
    }
  }
}

// Warnings
const warnings = report.warnings || [];
if (warnings.length > 0) {
  console.log(`Warnings:`);
  for (const w of warnings) {
    console.log(`  ! ${w}`);
  }
}

// ── Save ────────────────────────────────────────────────────────────────

const savedBytes = dest.save();
writeFileSync(tempDestPath, savedBytes);
rmSync(destPath, { force: true });
renameSync(tempDestPath, destPath);
console.log(`\nSaved: ${destPath} (${savedBytes.length} bytes)`);

// ── Verify with diff ────────────────────────────────────────────────────
// Reload the saved file and run a normalised diff to confirm the conversion.

const reloaded = new AudioFile(new Uint8Array(readFileSync(destPath)), basename(destPath));
const diff = source.diffTagsNormalized(reloaded);

console.log("\n── Verification (normalised diff) ──");
if (diff.isIdentical()) {
  console.log("  All normalised fields match.");
} else {
  const data = diff.toJson();
  if (data.changed.length > 0) {
    console.log(`  Changed: ${data.changed.length}`);
    for (const c of data.changed) {
      console.log(`    ${c.key}: ${JSON.stringify(c.left)} → ${JSON.stringify(c.right)}`);
    }
  }
  if (data.left_only.length > 0) {
    console.log(`  Source only: ${data.left_only.length}`);
    for (const e of data.left_only) {
      console.log(`    ${e.key}: ${JSON.stringify(e.values)}`);
    }
  }
  if (data.right_only.length > 0) {
    console.log(`  Dest only: ${data.right_only.length}`);
    for (const e of data.right_only) {
      console.log(`    ${e.key}: ${JSON.stringify(e.values)}`);
    }
  }
}

// ── Selective conversion (title + artist only) ──────────────────────────
// importTagsFromWithOptions() gives control over which fields to transfer.

console.log("\n── Selective Conversion (title + artist only) ──");
const dest2 = new AudioFile(new Uint8Array(readFileSync(destPath)), basename(destPath));

const report2 = dest2.importTagsFromWithOptions(
  source,
  ["Title", "Artist"],  // include_fields — only these standard fields
  null,                  // exclude_fields
  false,                 // transfer_custom — skip non-standard fields
);

const transferred2 = report2.transferred || [];
console.log(`Transferred: ${transferred2.length} fields`);
for (const field of transferred2) {
  console.log(`  + ${field}`);
}

diff.free();
dest2.free();
source.free();
dest.free();
reloaded.free();
