import type { ModelsDevSyncState } from "@/types/usage";
import { invoke } from "@tauri-apps/api/core";
export interface ModelsDevSyncResult {
  skipped: boolean;
  selected: number;
  imported: number;
  changed: number;
  syncedAt: number | null;
}
export const MODELS_DEV_SYNC_CONFIG_QUERY_KEY = [
  "models-dev-sync-config",
] as const;
export const MODELS_DEV_STARTUP_SYNC_INTERVAL_MS = 6 * 60 * 60 * 1000;
export function syncModelsDevPricing(
  _state?: ModelsDevSyncState,
  force = false,
): Promise<ModelsDevSyncResult> {
  return invoke("sync_models_dev_pricing", { force });
}
let startupSync: Promise<ModelsDevSyncResult> | null = null;
export function syncModelsDevPricingOnStartup(): Promise<ModelsDevSyncResult> {
  startupSync ??= syncModelsDevPricing();
  return startupSync;
}
