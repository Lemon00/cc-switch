import { QueryClient } from "@tanstack/react-query";
import { afterEach, describe, expect, it } from "vitest";
import { invalidateProviderCaches } from "@/lib/query/providerCache";
import { invalidateProfileCaches } from "@/lib/query/profiles";
const clients: QueryClient[] = [];
function client() {
  const result = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  clients.push(result);
  return result;
}
afterEach(() => clients.splice(0).forEach((value) => value.clear()));
describe("provider cache consistency", () => {
  it.each(["switch", "mode", "update", "profile"] as const)(
    "refreshes the default Claude plan after %s can change the native login",
    async (change) => {
      const cache = client();
      const plan = ["claude_official", "default_plan"];
      const status = ["managed-auth-status", "claude_official"];
      cache.setQueryData(plan, { subscriptionType: "pro" });
      cache.setQueryData(status, { accounts: [] });
      const otherAccount = ["claude_official", "quota", "unrelated"];
      cache.setQueryData(otherAccount, { success: true });
      if (change === "profile") await invalidateProfileCaches(cache, "claude");
      else await invalidateProviderCaches(cache, "claude", change);
      expect(cache.getQueryState(plan)?.isInvalidated).toBe(true);
      expect(cache.getQueryState(status)?.isInvalidated).toBe(true);
      expect(cache.getQueryState(otherAccount)?.isInvalidated).toBe(false);
    },
  );
  it.each(["add", "update", "delete"] as const)(
    "refreshes failover views after %s, within the affected app",
    async (change) => {
      const cache = client();
      const keys = [
        ["availableProvidersForFailover", "claude"],
        ["failoverQueue", "claude"],
      ];
      keys.forEach((key) => cache.setQueryData(key, []));
      cache.setQueryData(["failoverQueue", "codex"], []);
      await invalidateProviderCaches(cache, "claude", change);
      keys.forEach((key) =>
        expect(cache.getQueryState(key)?.isInvalidated).toBe(true),
      );
      expect(
        cache.getQueryState(["failoverQueue", "codex"])?.isInvalidated,
      ).toBe(false);
    },
  );
});
