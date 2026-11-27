#!/usr/bin/env node
// Generates the Tauri updater manifest (latest.json) from signed release
// artifacts in <assets-dir>. See scripts/README.md.
//
// Usage: node scripts/generate-updater-manifest.mjs <assets-dir> <tag> <base-url> [output] [pub-date] [notes]

import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  assetUrl,
  normalizeBaseUrl,
  validateTag,
} from "./release-manifest-utils.mjs";

const [
  assetsDir,
  tag,
  baseUrl,
  output = "latest.json",
  pubDateArg,
  notesArg,
] = process.argv.slice(2);

if (!assetsDir || !tag || !baseUrl || process.argv.length > 8) {
  console.error(
    "Usage: node scripts/generate-updater-manifest.mjs <assets-dir> <tag> <base-url> [output] [pub-date] [notes]",
  );
  process.exit(1);
}

const pubDate = pubDateArg !== undefined ? new Date(pubDateArg) : new Date();
if (Number.isNaN(pubDate.getTime())) {
  console.error(`Invalid pub-date: ${pubDateArg}`);
  process.exit(1);
}

const PLATFORMS = [
  { key: "windows-x86_64", suffix: "-Windows.msi" },
  { key: "linux-x86_64-deb", suffix: "-Linux-x86_64.deb" },
];

function isRegularFile(path) {
  return existsSync(path) && statSync(path).isFile();
}

try {
  validateTag(tag);
  const normalizedBase = normalizeBaseUrl(baseUrl);
  const assetPrefix = `CC-Switch-${tag.replaceAll("/", "-")}`;
  const platforms = {};
  const missing = [];

  for (const { key, suffix } of PLATFORMS) {
    const assetName = `${assetPrefix}${suffix}`;
    const assetPath = join(assetsDir, assetName);
    const sigPath = `${assetPath}.sig`;
    if (!isRegularFile(assetPath) || !isRegularFile(sigPath)) {
      missing.push(key);
      continue;
    }
    const signature = readFileSync(sigPath, "utf8").trim();
    if (!signature) {
      missing.push(key);
      continue;
    }
    platforms[key] = {
      signature,
      url: assetUrl(normalizedBase, tag, assetName),
    };
  }

  if (missing.length > 0) {
    throw new Error(
      `Missing updater artifact or signature for ${tag}: ${missing.join(", ")}`,
    );
  }

  const notes = notesArg !== undefined ? notesArg : `Release ${tag}`;
  if (!notes.trim()) {
    throw new Error("Release notes must not be empty");
  }

  const manifest = {
    version: tag.replace(/^v/, ""),
    notes,
    pub_date: pubDate.toISOString(),
    platforms,
  };

  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Wrote ${output} with ${PLATFORMS.length} platforms for ${tag}`,
  );
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
