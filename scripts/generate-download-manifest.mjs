#!/usr/bin/env node
// Generates manifest.json for this repository's release downloads, including
// the standalone CLI/TUI and Linux GUI portable archive. See scripts/README.md.
//
// Usage: node scripts/generate-download-manifest.mjs <assets-dir> <tag> <base-url> [output] [pub-date]

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  assetUrl,
  normalizeBaseUrl,
  validateTag,
} from "./release-manifest-utils.mjs";

const [assetsDir, tag, baseUrl, output = "manifest.json", pubDateArg] =
  process.argv.slice(2);

if (!assetsDir || !tag || !baseUrl || process.argv.length > 7) {
  console.error(
    "Usage: node scripts/generate-download-manifest.mjs <assets-dir> <tag> <base-url> [output] [pub-date]",
  );
  process.exit(1);
}

// Prefer the release's real publishedAt (passed by CI) over generation time.
const pubDate = pubDateArg !== undefined ? new Date(pubDateArg) : new Date();
if (Number.isNaN(pubDate.getTime())) {
  console.error(`Invalid pub-date: ${pubDateArg}`);
  process.exit(1);
}

// Longer suffixes must come before their shorter counterparts
// (e.g. -Windows-arm64.msi before -Windows.msi).
const RULES = [
  {
    suffix: "-Ubuntu-x86_64.tar.gz",
    platform: "linux",
    kind: "portable",
    arch: "x64",
  },
  {
    suffix: "-Ubuntu-arm64.tar.gz",
    platform: "linux",
    kind: "portable",
    arch: "arm64",
  },
  { suffix: "-macOS.dmg", platform: "macos", kind: "dmg", arch: "universal" },
  { suffix: "-macOS.zip", platform: "macos", kind: "zip", arch: "universal" },
  {
    suffix: "-Windows-arm64-Portable.zip",
    platform: "windows",
    kind: "portable",
    arch: "arm64",
  },
  {
    suffix: "-Windows-Portable.zip",
    platform: "windows",
    kind: "portable",
    arch: "x64",
  },
  {
    suffix: "-Windows-arm64.msi",
    platform: "windows",
    kind: "msi",
    arch: "arm64",
  },
  { suffix: "-Windows.msi", platform: "windows", kind: "msi", arch: "x64" },
  {
    suffix: "-Linux-arm64.AppImage",
    platform: "linux",
    kind: "appimage",
    arch: "arm64",
  },
  {
    suffix: "-Linux-x86_64.AppImage",
    platform: "linux",
    kind: "appimage",
    arch: "x64",
  },
  { suffix: "-Linux-arm64.deb", platform: "linux", kind: "deb", arch: "arm64" },
  { suffix: "-Linux-x86_64.deb", platform: "linux", kind: "deb", arch: "x64" },
  { suffix: "-Linux-arm64.rpm", platform: "linux", kind: "rpm", arch: "arm64" },
  { suffix: "-Linux-x86_64.rpm", platform: "linux", kind: "rpm", arch: "x64" },
];

try {
  validateTag(tag);
  const normalizedBase = normalizeBaseUrl(baseUrl);
  const assetPrefix = `CC-Switch-${tag.replaceAll("/", "-")}`;
  const files = [];

  for (const entry of readdirSync(assetsDir, { withFileTypes: true }).sort(
    (a, b) => a.name.localeCompare(b.name, "en"),
  )) {
    if (!entry.isFile()) continue;
    const name = entry.name;
    // Match this tag's download names exactly. Updater archives, signatures,
    // other releases and manifest files are not separate user downloads.
    const rule =
      name === "cc-switch-cli"
        ? { platform: "linux", kind: "cli", arch: "x64" }
        : name === "cc-switch-cli.exe"
          ? { platform: "windows", kind: "cli", arch: "x64" }
          : RULES.find((rule) => name === `${assetPrefix}${rule.suffix}`);
    if (!rule) continue;
    const path = join(assetsDir, name);
    const content = readFileSync(path);
    files.push({
      platform: rule.platform,
      kind: rule.kind,
      arch: rule.arch,
      name,
      size: content.length,
      sha256: createHash("sha256").update(content).digest("hex"),
      url: assetUrl(normalizedBase, tag, name),
    });
  }

  if (files.length === 0) {
    console.error(`No release assets matched in ${assetsDir}`);
    process.exit(1);
  }

  const manifest = {
    version: tag.replace(/^v/, ""),
    tag,
    pubDate: pubDate.toISOString(),
    files,
  };

  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Wrote ${output} with ${files.length} files for ${tag}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
