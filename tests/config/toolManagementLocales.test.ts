import { describe, expect, it } from "vitest";
import zh from "@/i18n/locales/zh.json";
const requiredKeys = [
  "manualInstallCommands",
  "updateAllTools",
  "currentVersion",
  "latestVersion",
  "updateAvailableShort",
  "toolInstall",
  "toolUpdate",
  "toolReady",
  "toolActionDone",
  "toolActionPartial",
  "toolActionFailed",
  "toolActionInProgress",
  "toolActionInProgressDetail",
  "toolNotRunnable",
  "toolActionVersionUnchangedTitle",
  "toolActionVersionUnchanged",
  "toolActionInstalledNotRunnable",
  "installedNotRunnable",
  "toolCheckEnv",
  "toolDiagnose",
  "toolDiagnosing",
  "toolConflictTitle",
  "toolConflictHint",
  "toolConflictDefault",
  "toolConflictNotRunnable",
  "toolDiagnoseNoConflict",
  "toolDiagnoseFailed",
  "toolUpgradeConfirmTitle",
  "toolUpgradeConfirmHint",
  "toolUpgradeWillRun",
  "toolUpgradeConfirmBtn",
  "toolUpgradeUnanchoredHint",
] as const;
type SettingsTranslations = Record<string, unknown>;
const locales = [["zh", zh.settings]] as const;
function interpolationVariables(value: string): string[] {
  return Array.from(
    value.matchAll(/\{\{([^}]+)\}\}/g),
    ([, name]) => name,
  ).sort();
}
describe("About tool management locale coverage", () => {
  it.each(locales)(
    "defines every tool management key in %s",
    (_locale, settings) => {
      const missing = requiredKeys.filter((key) => {
        const value = (settings as SettingsTranslations)[key];
        return typeof value !== "string" || value.trim().length === 0;
      });
      expect(missing).toEqual([]);
    },
  );
  it.each(locales.slice(1))(
    "preserves interpolation variables in %s",
    (_locale, settings) => {
      for (const key of requiredKeys) {
        const expected = zh.settings[key];
        const actual = (settings as SettingsTranslations)[key];
        expect(typeof actual).toBe("string");
        expect(interpolationVariables(actual as string)).toEqual(
          interpolationVariables(expected),
        );
      }
    },
  );
});
