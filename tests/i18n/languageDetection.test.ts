import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
let loadedI18n:
  | { changeLanguage: (language: string) => Promise<unknown> }
  | undefined;
beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
});
afterEach(async () => {
  await loadedI18n?.changeLanguage("zh");
  loadedI18n = undefined;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});
describe("bilingual startup language", () => {
  it.each([
    ["zh-CN", "zh"],
    ["zh-TW", "zh"],
    ["zh_Hant", "zh"],
    ["en-US", "en"],
    ["tr-TR", "en"],
    ["ja", "en"],
    ["fr", "en"],
    ["", "zh"],
  ])("detects %s as %s", async (language, expected) => {
    vi.stubGlobal("navigator", { language });
    const { default: i18n } = await import("@/i18n");
    loadedI18n = i18n;
    expect(i18n.language).toBe(expected);
    expect(i18n.t("common.save")).toBe(expected === "zh" ? "保存" : "Save");
  });
  it.each(["zh", "en"])(
    "keeps cached %s over the system language",
    async (language) => {
      localStorage.setItem("language", language);
      vi.stubGlobal("navigator", {
        language: language === "zh" ? "en-US" : "zh-CN",
      });
      const { default: i18n } = await import("@/i18n");
      loadedI18n = i18n;
      expect(i18n.language).toBe(language);
    },
  );
  it("uses the system language when cached language was retired", async () => {
    localStorage.setItem("language", "tr");
    vi.stubGlobal("navigator", { language: "zh-CN" });
    const { default: i18n } = await import("@/i18n");
    loadedI18n = i18n;
    expect(i18n.language).toBe("zh");
  });
  it("detects English when storage is inaccessible", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("unavailable");
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.stubGlobal("navigator", { languages: ["en-GB"] });
    const { default: i18n } = await import("@/i18n");
    loadedI18n = i18n;
    expect(i18n.language).toBe("en");
  });
});
