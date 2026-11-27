// Shared URL rules for downloadable assets and signed updater manifests.
export function validateTag(tag) {
  if (
    !tag ||
    tag.trim() !== tag ||
    /[\\\x00-\x1f\x7f]/.test(tag) ||
    tag.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error(`Invalid release tag: ${tag}`);
  }
}

export function normalizeBaseUrl(value) {
  const url = new URL(value);
  if (
    !["https:", "http:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(
      "Base URL must be an HTTP(S) URL without credentials, query or fragment",
    );
  }
  return url.href.replace(/\/+$/, "");
}

export function assetUrl(baseUrl, tag, name) {
  return `${baseUrl}/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}

export function githubAssetName(value, tag) {
  const url = new URL(value);
  const parts = url.pathname.split("/");
  if (
    url.origin !== "https://github.com" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    parts.length !== 7 ||
    !parts[1] ||
    !parts[2] ||
    parts[3] !== "releases" ||
    parts[4] !== "download" ||
    decodeURIComponent(parts[5]) !== tag
  ) {
    throw new Error(`Unexpected GitHub release asset URL for ${tag}: ${value}`);
  }
  const name = decodeURIComponent(parts[6]);
  if (
    !name ||
    name === "." ||
    name === ".." ||
    /[/\\\x00-\x1f\x7f]/.test(name)
  ) {
    throw new Error(`Invalid release asset filename: ${value}`);
  }
  return name;
}
