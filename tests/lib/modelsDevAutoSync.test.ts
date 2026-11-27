import { beforeEach, describe, expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import {
  syncModelsDevPricing,
  syncModelsDevPricingOnStartup,
} from "@/lib/modelsDevAutoSync";
import { fetchModelsDev } from "@/lib/modelsDev";
describe("models.dev backend synchronization", () => {
  beforeEach(() => {
    invoke.mockReset();
  });
  it("receives only the synchronization result from Rust", async () => {
    const result = {
      skipped: false,
      selected: 2,
      imported: 2,
      changed: 1,
      syncedAt: 123,
    };
    invoke.mockResolvedValue(result);
    expect(await syncModelsDevPricing()).toEqual(result);
    expect(invoke).toHaveBeenCalledWith("sync_models_dev_pricing", {
      force: false,
    });
    expect(invoke).toHaveBeenCalledOnce();
  });
  it("passes a manual force request to Rust", async () => {
    invoke.mockResolvedValue({ skipped: false });
    await syncModelsDevPricing(undefined, true);
    expect(invoke).toHaveBeenCalledWith("sync_models_dev_pricing", {
      force: true,
    });
  });
  it("starts the background synchronization once", async () => {
    invoke.mockResolvedValue({ skipped: true });
    const first = syncModelsDevPricingOnStartup();
    expect(syncModelsDevPricingOnStartup()).toBe(first);
    await first;
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("sync_models_dev_pricing", {
      force: false,
    });
  });
  it("queries the compact catalog only when requested", async () => {
    const catalog = {
      google: { models: { "gemini-flash": { limit: { context: 1048576 } } } },
    };
    invoke.mockResolvedValue(catalog);
    expect(await fetchModelsDev()).toEqual(catalog);
    expect(invoke).toHaveBeenCalledWith("fetch_models_dev");
  });
  it("propagates a backend sync error", async () => {
    invoke.mockRejectedValue(new Error("offline"));
    await expect(syncModelsDevPricing()).rejects.toThrow("offline");
  });
});
