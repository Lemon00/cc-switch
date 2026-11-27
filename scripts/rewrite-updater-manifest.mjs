#!/usr/bin/env node
// Rewrites the Tauri updater manifest (latest.json) so its download URLs point
// at an explicitly supplied mirror instead of GitHub Releases. Signatures cover file
// contents, not URLs, so they stay valid unchanged — the updater still verifies
// every downloaded artifact against the pubkey baked into the app.
//
// Usage: node scripts/rewrite-updater-manifest.mjs <latest-json> <tag> <base-url> [output]

import { readFileSync, writeFileSync } from "node:fs";
import {
  assetUrl,
  githubAssetName,
  normalizeBaseUrl,
  validateTag,
} from "./release-manifest-utils.mjs";

const [input, tag, baseUrl, output = "latest-r2.json"] = process.argv.slice(2);

if (!input || !tag || !baseUrl || process.argv.length > 6) {
  console.error(
    "Usage: node scripts/rewrite-updater-manifest.mjs <latest-json> <tag> <base-url> [output]",
  );
  process.exit(1);
}

try {
  validateTag(tag);
  const normalizedBase = normalizeBaseUrl(baseUrl);
  const manifest = JSON.parse(
    readFileSync(input, "utf8").replace(/^\uFEFF/, ""),
  );
  if (
    !manifest ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    manifest.version !== tag.replace(/^v/, "")
  ) {
    throw new Error(`Manifest version does not match release tag ${tag}`);
  }
  if (
    !manifest.platforms ||
    typeof manifest.platforms !== "object" ||
    Array.isArray(manifest.platforms)
  ) {
    throw new Error(`Invalid platforms in ${input}`);
  }
  const platforms = Object.entries(manifest.platforms);
  if (platforms.length === 0) {
    console.error(`No platforms found in ${input}`);
    process.exit(1);
  }

  // Mirrored artifacts live at <base>/<tag>/<file>; on GitHub they live at
  // .../releases/download/<tag>/<file>. Rebuild from the filename rather than
  // prefix-replacing so a layout change on either side fails loudly here.
  for (const [key, entry] of platforms) {
    if (
      typeof entry?.url !== "string" ||
      typeof entry?.signature !== "string" ||
      !entry.signature.trim()
    ) {
      console.error(`Platform ${key} is missing url or signature`);
      process.exit(1);
    }
    const name = githubAssetName(entry.url, tag);
    entry.url = assetUrl(normalizedBase, tag, name);
  }

  writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(
    `Wrote ${output} with ${platforms.length} platforms pointing at ${normalizedBase}/${encodeURIComponent(tag)}/`,
  );
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
