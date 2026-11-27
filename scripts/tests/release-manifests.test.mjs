import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scripts = fileURLToPath(new URL("../", import.meta.url));
const tag = "v1.0.0";
const base = "https://github.com/Lemon00/cc-switch/releases/download";
const pubDate = "2026-11-26T08:00:00Z";

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "cc-switch-manifests-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const assets = join(dir, "assets");
  mkdirSync(assets);
  return { dir, assets, output: join(dir, "output.json") };
}

function run(script, args) {
  const result = spawnSync(process.execPath, [join(scripts, script), ...args], {
    encoding: "utf8",
    timeout: 15_000,
  });
  assert.ifError(result.error);
  return result;
}

function generate(f, releaseTag = tag, url = base) {
  return run("generate-download-manifest.mjs", [
    f.assets,
    releaseTag,
    url,
    f.output,
    pubDate,
  ]);
}

function generateUpdater(f, releaseTag = tag, url = base, notes) {
  const args = [f.assets, releaseTag, url, f.output, pubDate];
  if (notes !== undefined) args.push(notes);
  return run("generate-updater-manifest.mjs", args);
}

function updater() {
  return {
    version: "1.0.0",
    notes: "Release notes",
    pub_date: pubDate,
    platforms: {
      "windows-x86_64": {
        url: `${base}/${tag}/CC-Switch-${tag}-Windows.msi`,
        signature: "windows-signature\n",
      },
      "linux-x86_64-deb": {
        url: `${base}/${tag}/CC-Switch-${tag}-Linux-x86_64.deb`,
        signature: "linux-signature\n",
        extra: "keep",
      },
    },
  };
}

test("download manifest includes every current release download and its real hash", (t) => {
  const f = fixture(t);
  const names = [
    ["CC-Switch-v1.0.0-Windows.msi", "windows", "msi"],
    ["CC-Switch-v1.0.0-Windows-Portable.zip", "windows", "portable"],
    ["CC-Switch-v1.0.0-Linux-x86_64.deb", "linux", "deb"],
    ["CC-Switch-v1.0.0-Ubuntu-x86_64.tar.gz", "linux", "portable"],
    ["cc-switch-cli", "linux", "cli"],
  ];
  for (const [name] of names)
    writeFileSync(join(f.assets, name), `artifact: ${name}`);
  for (const name of [
    "CC-Switch-v0.9.0-Windows.msi",
    "CC-Switch-v1.0.0-Linux-x86_64.deb.sig",
    "CC-Switch-v1.0.0-Linux-x86_64.AppImage.tar.gz",
    "CLI-TUI-README.md",
    "latest.json",
    "manifest.json",
  ]) {
    writeFileSync(join(f.assets, name), "ignored");
  }
  mkdirSync(join(f.assets, "CC-Switch-v1.0.0-macOS.dmg"));
  const result = generate(f, tag, `${base}///`);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(readFileSync(f.output, "utf8"));
  assert.equal(manifest.version, "1.0.0");
  assert.equal(manifest.tag, tag);
  assert.equal(manifest.pubDate, "2026-11-26T08:00:00.000Z");
  assert.equal(manifest.files.length, names.length);
  for (const [name, platform, kind] of names) {
    const file = manifest.files.find((file) => file.name === name);
    const content = readFileSync(join(f.assets, name));
    assert.deepEqual(file, {
      platform,
      kind,
      arch: "x64",
      name,
      size: content.length,
      sha256: createHash("sha256").update(content).digest("hex"),
      url: `${base}/${tag}/${name}`,
    });
  }
});

test("standalone CLI-only releases work, including optional Windows builds", (t) => {
  const f = fixture(t);
  writeFileSync(join(f.assets, "cc-switch-cli.exe"), "windows CLI/TUI");
  const result = generate(f);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(readFileSync(f.output, "utf8"));
  assert.equal(manifest.files.length, 1);
  assert.equal(manifest.files[0].platform, "windows");
  assert.equal(manifest.files[0].kind, "cli");
});

test("tag path segments and filenames are URL encoded without losing release identity", (t) => {
  const f = fixture(t);
  const releaseTag = "v1.0.0/test #1";
  const name = "CC-Switch-v1.0.0-test #1-Ubuntu-x86_64.tar.gz";
  writeFileSync(join(f.assets, name), "portable");
  const result = generate(f, releaseTag);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(readFileSync(f.output, "utf8"));
  assert.equal(
    manifest.files[0].url,
    `${base}/v1.0.0%2Ftest%20%231/${encodeURIComponent(name)}`,
  );
});

test("invalid download inputs fail without overwriting an existing output", (t) => {
  const f = fixture(t);
  writeFileSync(f.output, "keep previous manifest");
  assert.notEqual(generate(f).status, 0);
  writeFileSync(join(f.assets, "cc-switch-cli"), "CLI");
  for (const url of [
    "not-a-url",
    "file:///tmp/downloads",
    "https://example.test/?query=1",
    "https://user:secret@example.test",
  ]) {
    assert.notEqual(generate(f, tag, url).status, 0, url);
  }
  for (const releaseTag of ["..", "v1/../bad", " v1.0.0", "v1\\bad"]) {
    assert.notEqual(generate(f, releaseTag).status, 0, releaseTag);
  }
  for (const badDate of ["bad-date", ""]) {
    const result = run("generate-download-manifest.mjs", [
      f.assets,
      tag,
      base,
      f.output,
      badDate,
    ]);
    assert.notEqual(result.status, 0, JSON.stringify(badDate));
  }
  assert.equal(readFileSync(f.output, "utf8"), "keep previous manifest");
});

test("rewriting updater URLs preserves signatures, platform keys and metadata, including BOM input", (t) => {
  const f = fixture(t);
  const input = join(f.dir, "latest.json");
  const original = updater();
  writeFileSync(input, `\uFEFF${JSON.stringify(original)}`);
  const result = run("rewrite-updater-manifest.mjs", [
    input,
    tag,
    "https://mirror.example/downloads/",
    f.output,
  ]);
  assert.equal(result.status, 0, result.stderr);
  const rewritten = JSON.parse(readFileSync(f.output, "utf8"));
  for (const [key, entry] of Object.entries(original.platforms)) {
    original.platforms[key].url = entry.url.replace(
      base,
      "https://mirror.example/downloads",
    );
  }
  assert.deepEqual(rewritten, original);
  assert.ok(readFileSync(input, "utf8").includes("github.com"));
});

test("updater rewrite rejects inconsistent tags, missing signatures and misleading asset URLs atomically", (t) => {
  const f = fixture(t);
  const input = join(f.dir, "latest.json");
  writeFileSync(f.output, "keep previous manifest");
  const broken = [];
  const wrongVersion = updater();
  wrongVersion.version = "9.0.0";
  broken.push(
    wrongVersion,
    { version: "1.0.0", platforms: [] },
    { version: "1.0.0", platforms: {} },
  );
  const noSignature = updater();
  noSignature.platforms["linux-x86_64-deb"].signature = " ";
  broken.push(noSignature);
  for (const url of [
    `${base}/v0.9.0/file.deb`,
    `https://github.com/not-an-asset?path=/releases/download/${tag}/file.deb`,
    `https://github.com.evil.test/Lemon00/cc-switch/releases/download/${tag}/file.deb`,
    `${base}/${tag}/file.deb/extra`,
    `${base}/${tag}/`,
    `${base}/${tag}/%2Ffile.deb`,
    `${base}/${tag}/file.deb#fragment`,
  ]) {
    const manifest = updater();
    // The first valid platform may already be processed; output still must not change.
    manifest.platforms["linux-x86_64-deb"].url = url;
    broken.push(manifest);
  }
  for (const manifest of broken) {
    writeFileSync(input, JSON.stringify(manifest));
    const result = run("rewrite-updater-manifest.mjs", [
      input,
      tag,
      "https://mirror.example",
      f.output,
    ]);
    assert.notEqual(result.status, 0, JSON.stringify(manifest));
    assert.equal(readFileSync(f.output, "utf8"), "keep previous manifest");
  }
});

test("updater URLs handle encoded tags and filenames exactly once", (t) => {
  const f = fixture(t);
  const input = join(f.dir, "latest.json");
  const manifest = updater();
  manifest.version = "1.0.0/test";
  manifest.platforms = {
    "windows-x86_64": {
      url: `${base}/v1.0.0%2Ftest/CC%20Switch.msi`,
      signature: "signature",
    },
  };
  writeFileSync(input, JSON.stringify(manifest));
  const result = run("rewrite-updater-manifest.mjs", [
    input,
    "v1.0.0/test",
    "https://mirror.example",
    f.output,
  ]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(
    JSON.parse(readFileSync(f.output, "utf8")).platforms["windows-x86_64"].url,
    "https://mirror.example/v1.0.0%2Ftest/CC%20Switch.msi",
  );
});

test("updater manifest generator matches exact tag artifacts, encodes URLs and round-trips through mirror rewrite", (t) => {
  const f = fixture(t);
  const releaseTag = "v1.0.0/test #1";
  const safeTag = "v1.0.0-test #1";
  const winName = `CC-Switch-${safeTag}-Windows.msi`;
  const linuxName = `CC-Switch-${safeTag}-Linux-x86_64.deb`;
  writeFileSync(join(f.assets, winName), "win-msi");
  writeFileSync(join(f.assets, `${winName}.sig`), "win-sig\r\n");
  writeFileSync(join(f.assets, linuxName), "linux-deb");
  writeFileSync(join(f.assets, `${linuxName}.sig`), "linux-sig\n");
  writeFileSync(
    join(f.assets, "CC-Switch-v0.9.0-Windows.msi.sig"),
    "stale-sig",
  );

  const genResult = generateUpdater(f, releaseTag, `${base}/`);
  assert.equal(genResult.status, 0, genResult.stderr);
  const generated = JSON.parse(readFileSync(f.output, "utf8"));
  assert.deepEqual(generated, {
    version: "1.0.0/test #1",
    notes: `Release ${releaseTag}`,
    pub_date: "2026-11-26T08:00:00.000Z",
    platforms: {
      "windows-x86_64": {
        signature: "win-sig",
        url: `${base}/v1.0.0%2Ftest%20%231/${encodeURIComponent(winName)}`,
      },
      "linux-x86_64-deb": {
        signature: "linux-sig",
        url: `${base}/v1.0.0%2Ftest%20%231/${encodeURIComponent(linuxName)}`,
      },
    },
  });

  const mirroredPath = join(f.dir, "latest-mirror.json");
  const rewriteResult = run("rewrite-updater-manifest.mjs", [
    f.output,
    releaseTag,
    "https://mirror.example/downloads/",
    mirroredPath,
  ]);
  assert.equal(rewriteResult.status, 0, rewriteResult.stderr);
  const mirrored = JSON.parse(readFileSync(mirroredPath, "utf8"));
  assert.equal(
    mirrored.platforms["windows-x86_64"].url,
    `https://mirror.example/downloads/v1.0.0%2Ftest%20%231/${encodeURIComponent(winName)}`,
  );
  assert.equal(
    mirrored.platforms["linux-x86_64-deb"].url,
    `https://mirror.example/downloads/v1.0.0%2Ftest%20%231/${encodeURIComponent(linuxName)}`,
  );
});

test("updater manifest generator rejects missing platforms, orphan signatures, empty signatures and bad dates without overwriting output", (t) => {
  const f = fixture(t);
  writeFileSync(f.output, "keep previous latest");

  writeFileSync(join(f.assets, "CC-Switch-v1.0.0-Windows.msi"), "win-msi");
  writeFileSync(
    join(f.assets, "CC-Switch-v1.0.0-Windows.msi.sig"),
    "win-sig",
  );
  assert.notEqual(generateUpdater(f).status, 0);

  writeFileSync(
    join(f.assets, "CC-Switch-v1.0.0-Linux-x86_64.deb.sig"),
    "linux-sig",
  );
  assert.notEqual(generateUpdater(f).status, 0);

  writeFileSync(
    join(f.assets, "CC-Switch-v1.0.0-Linux-x86_64.deb"),
    "linux-deb",
  );
  writeFileSync(join(f.assets, "CC-Switch-v1.0.0-Linux-x86_64.deb.sig"), " \n");
  assert.notEqual(generateUpdater(f).status, 0);

  writeFileSync(
    join(f.assets, "CC-Switch-v1.0.0-Linux-x86_64.deb.sig"),
    "linux-sig",
  );
  for (const badDate of ["bad-date", ""]) {
    const result = run("generate-updater-manifest.mjs", [
      f.assets,
      tag,
      base,
      f.output,
      badDate,
    ]);
    assert.notEqual(result.status, 0, JSON.stringify(badDate));
  }
  assert.notEqual(generateUpdater(f, tag, base, "   ").status, 0);
  assert.equal(readFileSync(f.output, "utf8"), "keep previous latest");
});
