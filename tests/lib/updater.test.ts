import { beforeEach, describe, expect, it, vi } from "vitest";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { checkForUpdate } from "@/lib/updater";
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
function updateFixture() {
  return {
    version: "1.1.0",
    body: "Release notes from the updater plugin",
    date: "2026-11-23T00:00:00Z",
    close: vi.fn().mockResolvedValue(undefined),
  };
}
describe("checkForUpdate", () => {
  beforeEach(() => {
    vi.mocked(getVersion).mockReset().mockResolvedValue("1.0.0");
    vi.mocked(check).mockReset();
  });
  it("copies the plugin body into release notes and releases its resource", async () => {
    const update = updateFixture();
    vi.mocked(check).mockResolvedValue(update as unknown as Update);
    expect(await checkForUpdate({ timeout: 5000 })).toEqual({
      status: "available",
      info: {
        currentVersion: "1.0.0",
        availableVersion: update.version,
        notes: update.body,
        pubDate: update.date,
      },
    });
    expect(check).toHaveBeenCalledWith({ timeout: 5000 });
    expect(update.close).toHaveBeenCalledTimes(1);
  });
  it("does not retain resources across repeated update checks", async () => {
    const first = updateFixture();
    const second = updateFixture();
    vi.mocked(check)
      .mockResolvedValueOnce(first as unknown as Update)
      .mockResolvedValueOnce(second as unknown as Update);
    await checkForUpdate();
    await checkForUpdate();
    expect(first.close).toHaveBeenCalledTimes(1);
    expect(second.close).toHaveBeenCalledTimes(1);
  });
  it("keeps valid metadata when resource cleanup fails", async () => {
    const update = updateFixture();
    const failure = new Error("resource cleanup failed");
    update.close.mockRejectedValue(failure);
    vi.mocked(check).mockResolvedValue(update as unknown as Update);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      expect((await checkForUpdate()).status).toBe("available");
      expect(warn).toHaveBeenCalledWith(
        "Unable to release update check resource",
        failure,
      );
    } finally {
      warn.mockRestore();
    }
  });
  it("returns up-to-date when the plugin returns no resource", async () => {
    vi.mocked(check).mockResolvedValue(null);
    expect(await checkForUpdate()).toEqual({ status: "up-to-date" });
    expect(check).toHaveBeenCalledWith({ timeout: 30000 });
  });
  it("propagates check failures", async () => {
    const failure = new Error("network unavailable");
    vi.mocked(check).mockRejectedValue(failure);
    await expect(checkForUpdate()).rejects.toBe(failure);
  });
});
