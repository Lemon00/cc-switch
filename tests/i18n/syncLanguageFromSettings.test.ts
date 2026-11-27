import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";

describe("syncLanguageFromSettings", () => {
  beforeEach(() => {
    i18n.addResourceBundle("zh", "translation", zh, true, true);
    i18n.addResourceBundle("en", "translation", en, true, true);
  });
  afterEach(async () => {
    vi.restoreAllMocks();
    localStorage.removeItem("language");
    await i18n.changeLanguage("zh");
  });
  it.each(["zh", "en"])(
    "restores saved %s over a conflicting cache",
    async (language) => {
      const stale = language === "zh" ? "en" : "zh";
      await i18n.changeLanguage(stale);
      localStorage.setItem("language", stale);
      syncLanguageFromSettings(language);
      expect(i18n.language).toBe(language);
      expect(localStorage.getItem("language")).toBe(language);
      expect(i18n.t("common.save")).toBe(language === "zh" ? "保存" : "Save");
    },
  );
  it.each([
    ["zh-TW", "zh"],
    ["zh-Hant", "zh"],
    ["en-US", "en"],
    ["tr", "en"],
    ["ja", "en"],
  ])("normalizes legacy %s to %s", (language, expected) => {
    syncLanguageFromSettings(language);
    expect(i18n.language).toBe(expected);
    expect(localStorage.getItem("language")).toBe(expected);
  });
  it("refills an empty cache even when English is already active", async () => {
    await i18n.changeLanguage("en");
    localStorage.removeItem("language");
    syncLanguageFromSettings("en");
    expect(localStorage.getItem("language")).toBe("en");
  });
  it("preserves the detected language when no setting exists", async () => {
    await i18n.changeLanguage("en");
    syncLanguageFromSettings(undefined);
    syncLanguageFromSettings(null);
    expect(i18n.language).toBe("en");
  });
  it("still applies the saved language when storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("unavailable");
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(() => syncLanguageFromSettings("en")).not.toThrow();
    expect(i18n.language).toBe("en");
  });
});
