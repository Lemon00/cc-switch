import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh.json";
function flattenStrings(
  value: unknown,
  path: string[] = [],
  result = new Map<string, string>(),
): Map<string, string> {
  if (typeof value === "string") {
    result.set(path.join("."), value);
  } else if (typeof value === "object" && value !== null) {
    for (const [key, child] of Object.entries(value)) {
      flattenStrings(child, [...path, key], result);
    }
  }
  return result;
}
function interpolationVariables(value: string): string[] {
  return Array.from(
    value.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g),
    ([, name]) => name,
  ).sort();
}
const reference = flattenStrings(zh);
describe("Chinese locale", () => {
  it("keeps every existing translation non-empty", () => {
    expect(
      [...flattenStrings(zh)].filter(
        ([key, value]) => reference.get(key)?.trim() && !value.trim(),
      ),
    ).toEqual([]);
  });
  it("preserves interpolation variables in existing translations", () => {
    expect(
      [...flattenStrings(zh)]
        .filter(([key, value]) => {
          const expected = reference.get(key);
          return (
            expected !== undefined &&
            interpolationVariables(value).join("\0") !==
              interpolationVariables(expected).join("\0")
          );
        })
        .map(([key]) => key),
    ).toEqual([]);
  });
  it("preserves protocol fields, configuration literals, and commands", () => {
    const translations = flattenStrings(zh);
    const exactLiterals = [
      "opencode.extraOptionKeyPlaceholder",
      "opencode.modelExtraFieldKeyPlaceholder",
      "opencode.modelOptionKeyPlaceholder",
      "opencode.modelOptionValuePlaceholder",
      "opencode.providerKeyPlaceholder",
      "codexConfig.configToml",
      "providerPreset.plan.aksk",
      "mcp.envPlaceholder",
      "mcp.wizard.envPlaceholder",
      "skills.searchSource.skillssh",
    ];
    for (const key of exactLiterals) {
      expect(translations.get(key), key).toBe(reference.get(key));
    }
    expect(translations.get("usageScript.fieldRemaining")).toContain(
      "• remaining:",
    );
    expect(translations.get("usageScript.fieldUnit")).toContain("• unit:");
    expect(translations.get("usageScript.fieldTotal")).toContain("• total:");
    expect(translations.get("usageScript.fieldUsed")).toContain("• used:");
    expect(translations.get("usageScript.fieldExtra")).toContain("• extra:");
    expect(translations.get("settings.s3Sync.presets.r2Hint")).toContain(
      "auto",
    );
    expect(translations.get("settings.s3Sync.presets.cosHint")).toContain(
      "ap-beijing",
    );
    expect(
      translations.get("settings.manualUninstallBunLauncherHint"),
    ).toContain("OpenClaw Bun launcher");
    expect(translations.get("prompts.contentPlaceholderHermes")).toMatch(
      /^# SOUL\.md\n/,
    );
  });
});
