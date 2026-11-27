import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { parse } from "smol-toml";
import { describe, expect, it } from "vitest";
import { WHATS_NEW_ENTRIES } from "@/lib/whatsNew";
const read = (path: string) =>
  readFileSync(resolve(process.cwd(), path), "utf8");
const version: string = JSON.parse(read("package.json")).version;
describe("independent release metadata", () => {
  it("keeps desktop, core, CLI and their lockfiles on the same release", () => {
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe(version);
    for (const directory of [
      "src-tauri",
      "crates/cc-switch-core",
      "crates/cc-switch-cli",
    ]) {
      const manifest = parse(read(`${directory}/Cargo.toml`));
      expect(manifest.package).toHaveProperty("version", version);
    }
    for (const path of [
      "src-tauri/Cargo.lock",
      "crates/cc-switch-cli/Cargo.lock",
    ]) {
      const lock = parse(read(path));
      const packages = lock.package as {
        name: string;
        version: string;
      }[];
      const local = packages.filter((entry) =>
        /^cc-switch(?:-core|-cli)?$/.test(entry.name),
      );
      expect(local.length).toBe(2);
      for (const entry of local) expect(entry.version).toBe(version);
    }
  });
  it("ships the current summary and a release body for the workflow", () => {
    expect(WHATS_NEW_ENTRIES.some((entry) => entry.version === version)).toBe(
      true,
    );
    expect(read(`docs/release-notes/v${version}-zh.md`)).toContain(version);
    if (process.env.GITHUB_REF_TYPE === "tag") {
      expect(process.env.GITHUB_REF_NAME).toBe(`v${version}`);
    }
  });
});
