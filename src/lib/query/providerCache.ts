import { isProxyAppId } from "@/config/appCapabilities";
import type { AppId } from "@/lib/api/types";
import type { SubscriptionQuota } from "@/types/subscription";
import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { proxyKeys } from "./proxyKeys";
import { usageKeys } from "./usageKeys";
export type ProviderChange =
  | "add"
  | "update"
  | "delete"
  | "switch"
  | "mode"
  | "live";
/** Switching or reapplying a bound Claude card can replace the native login. */
export const claudeLoginQueryKeys = [
  ["managed-auth-status", "claude_official"],
  ["claude_official", "default_plan"],
] as const;
/** Shared by mutations, confirmation dialogs and backend/tray events. */
export async function invalidateProviderCaches(
  client: QueryClient,
  app: AppId,
  change: ProviderChange,
  providerIds: readonly string[] = [],
) {
  const keys: QueryKey[] = [["providers", app]];
  if (
    isProxyAppId(app) &&
    ["add", "update", "delete", "live"].includes(change)
  ) {
    keys.push(["availableProvidersForFailover", app], ["failoverQueue", app]);
  }
  if (app === "opencode") {
    keys.push(
      ["opencodeLiveProviderIds"],
      ["opencodeDefaultModel"],
      ["opencode", "runtime-models"],
    );
    for (const category of ["omo", "omo-slim"]) {
      keys.push([category, "current-provider-id"]);
      if (change === "add" || change === "delete") {
        keys.push([category, "provider-count"]);
      }
    }
  }
  if (change === "switch" || change === "mode") {
    keys.push(proxyKeys.status, proxyKeys.takeoverStatus);
  }
  if (app === "claude" && ["switch", "mode", "update"].includes(change)) {
    keys.push(...claudeLoginQueryKeys);
  }
  if (change === "update") {
    for (const id of new Set(providerIds)) keys.push(usageKeys.script(id, app));
  }
  // invalidateQueries alone reuses an initial request with no cached data.
  // Cancel first so a pre-write snapshot cannot clear the invalidation later.
  await Promise.all(keys.map((queryKey) => client.cancelQueries({ queryKey })));
  await Promise.all([
    ...keys.map((queryKey) => client.invalidateQueries({ queryKey })),
    ...(change === "switch"
      ? [
          client.invalidateQueries({
            predicate: (query) =>
              query.queryKey[0] === "codex_oauth" &&
              query.queryKey[1] === "quota" &&
              (query.state.status === "error" ||
                (query.state.data as SubscriptionQuota | undefined)?.success ===
                  false),
          }),
        ]
      : []),
  ]);
}
