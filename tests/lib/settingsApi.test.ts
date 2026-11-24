import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { backupsApi, settingsApi } from "@/lib/api/settings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("settings patches", () => {
  it("preserves backup restore warnings and supports legacy string responses", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      backupId: "safety.db",
      warning: "Prompt projection failed",
    });
    await expect(backupsApi.restoreDbBackup("selected.db")).resolves.toEqual({
      backupId: "safety.db",
      warning: "Prompt projection failed",
    });
    vi.mocked(invoke).mockResolvedValueOnce("legacy.db");
    await expect(backupsApi.restoreDbBackup("selected.db")).resolves.toEqual({
      backupId: "legacy.db",
    });
  });

  it("omits unspecified defaulted fields while explicitly clearing overrides", async () => {
    await settingsApi.save({
      showInTray: false,
      enableLocalProxy: undefined,
      skillSyncMethod: undefined,
      claudeConfigDir: undefined,
      codexStackSubagentModel: undefined,
    });
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: {
        showInTray: false,
        claudeConfigDir: null,
        codexStackSubagentModel: null,
      },
    });
  });
});
