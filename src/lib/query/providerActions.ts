import { hasNativeProviderCatalog } from "@/config/appCapabilities";
import { providersApi, type AppId } from "@/lib/api";
import type { QueryClient } from "@tanstack/react-query";
import { invalidateProviderCaches } from "./providerCache";
export async function refreshProviderTray() {
  try {
    await providersApi.updateTrayMenu();
  } catch (error) {
    console.error("Failed to update tray menu after provider change", error);
  }
}
/** The explicit app is captured by the caller before a confirmation/async action. */
export async function deleteProviderAndRefresh(
  client: QueryClient,
  app: AppId,
  id: string,
) {
  try {
    const deleted = await providersApi.delete(id, app);
    if (deleted) {
      await invalidateProviderCaches(client, app, "delete");
      await refreshProviderTray();
    }
    return deleted;
  } finally {
    await settleProviderChange(client, app);
  }
}
// A window can mount several consumers for the same app. Share only in-flight
// work within its QueryClient; completed refreshes and other windows stay independent.
interface NativeRefresh {
  promise: Promise<void>;
  refreshAgain: boolean;
}
const nativeRefreshes = new WeakMap<QueryClient, Map<AppId, NativeRefresh>>();
export function refreshNativeProviderCatalog(
  client: QueryClient,
  app: AppId,
  options: {
    afterChange?: boolean;
  } = {},
): Promise<void> {
  let pending = nativeRefreshes.get(client);
  if (!pending) {
    pending = new Map();
    nativeRefreshes.set(client, pending);
  }
  const existing = pending.get(app);
  if (existing) {
    // A write may have completed after the running refresh read native files.
    // Coalesce such writes into a trailing pass, not the older snapshot.
    if (options.afterChange) existing.refreshAgain = true;
    return existing.promise;
  }
  const entry: NativeRefresh = {
    promise: Promise.resolve(),
    refreshAgain: false,
  };
  const task = (async () => {
    try {
      do {
        entry.refreshAgain = false;
        let failure:
          | {
              error: unknown;
            }
          | undefined;
        try {
          await providersApi.refresh(app);
        } catch (error) {
          failure = { error };
        }
        // Reconciliation can partially succeed before encountering an unreadable file.
        await invalidateProviderCaches(client, app, "live");
        // A newer requested pass can recover from the previous pass's failure.
        if (!entry.refreshAgain && failure) throw failure.error;
      } while (entry.refreshAgain);
    } finally {
      // Remove in this continuation, before resolving the shared promise: a new
      // write must not join a finished pass in a later .finally microtask.
      pending.delete(app);
    }
  })();
  entry.promise = task;
  pending.set(app, entry);
  return task;
}
/** Preserve native readback after successful and failed mutations. */
export async function settleProviderChange(client: QueryClient, app: AppId) {
  if (!hasNativeProviderCatalog(app)) return;
  try {
    await refreshNativeProviderCatalog(client, app, { afterChange: true });
  } catch (error) {
    console.warn(
      `Failed to refresh native ${app} state after provider change`,
      error,
    );
  }
}
/** Explicitly refresh native state before reading its persisted snapshot. */
export async function refreshProviderLiveState(
  client: QueryClient,
  app: AppId,
) {
  if (hasNativeProviderCatalog(app)) {
    await refreshNativeProviderCatalog(client, app, { afterChange: true });
  } else {
    await invalidateProviderCaches(client, app, "live");
  }
}
