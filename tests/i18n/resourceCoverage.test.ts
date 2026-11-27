import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh.json";
import en from "@/i18n/locales/en.json";

function flatten(
  value: Record<string, unknown>,
  prefix = "",
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(value).flatMap(([key, text]) =>
      typeof text === "string"
        ? [[`${prefix}${key}`, text]]
        : Object.entries(
            flatten(text as Record<string, unknown>, `${prefix}${key}.`),
          ),
    ),
  );
}
describe("bilingual resources", () => {
  it("provides every Chinese key in English with matching interpolation parameters", () => {
    const chinese = flatten(zh);
    const english = flatten(en);
    expect(Object.keys(english).sort()).toEqual(Object.keys(chinese).sort());
    const params = (text: string) =>
      [...text.matchAll(/\{\{\s*-?\s*([^},]+)(?:,[^}]+)?\}\}/g)]
        .map((match) => match[1].trim())
        .sort();
    // These call sites supply both locale-specific forms (date/month/day,
    // count/more, and labels with or without spacing).
    const localizedParams: Record<string, string[]> = {
      "providerPreset.matchedVersions": ["more", "name"],
      "prompts.updatedOn": ["date"],
      "quota.tierLeft": ["label", "value"],
      "quota.tierUsedUp": ["label"],
      "quota.tierUsed": ["label", "value"],
    };
    for (const [key, text] of Object.entries(chinese)) {
      if (text.trim()) expect(english[key].trim(), key).not.toBe("");
      expect(params(english[key]), key).toEqual(
        localizedParams[key] ?? params(text),
      );
    }
  });
});
