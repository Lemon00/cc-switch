import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

describe("desktop window lifecycle configuration", () => {
  for (const filename of ["tauri.conf.json", "tauri.windows.conf.json"]) {
    it(`${filename} leaves main window creation to the lightweight lifecycle`, () => {
      const config = JSON.parse(
        readFileSync(path.resolve("src-tauri", filename), "utf8"),
      );
      // Platform windows arrays replace the base array; create=false must be
      // present in each one or Tauri starts a WebView before reading settings.
      const main = config.app.windows.find(
        (window: { label: string }) => window.label === "main",
      );
      expect(main).toBeDefined();
      expect(main.create).toBe(false);
      expect(main.visible).toBe(false);
    });
  }
});
