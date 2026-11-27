import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh.json";
describe("Antigravity locale parity", () => {
  const locales = { zh } as Record<string, unknown>;
  const collect = (node: unknown, path: string, out: string[]) => {
    if (node && typeof node === "object") {
      for (const [key, value] of Object.entries(node)) {
        collect(value, path ? `${path}.${key}` : key, out);
      }
    } else if (/(^|\.)agy/i.test(path)) {
      out.push(path);
    }
    return out;
  };
  const reference = collect(zh, "", []);
  it("has agy keys to compare", () => {
    expect(reference).toEqual(
      expect.arrayContaining([
        "apps.agy",
        "provider.form.agy.modelHelp",
        "sessionManager.agyDeleteHint",
      ]),
    );
  });
  it.each([])("%s defines every agy key", (lng) => {
    const own = new Set(collect(locales[lng], "", []));
    expect(reference.filter((key) => !own.has(key))).toEqual([]);
  });
});
