import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { afterEach, describe, expect, it } from "vitest";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import { invalidateProfileCaches } from "@/lib/query/profiles";
const clients: QueryClient[] = [];
afterEach(() => clients.splice(0).forEach((client) => client.clear()));
describe("queries after persisted changes", () => {
  it.each(["restore", "profile", "sort"])(
    "%s discards a cold read started before the write",
    async (change) => {
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      clients.push(client);
      const key = ["providers", "codex"];
      let finishOld!: (value: string) => void;
      let calls = 0;
      const observer = new QueryObserver(client, {
        queryKey: key,
        queryFn: () =>
          ++calls === 1
            ? new Promise<string>((resolve) => {
                finishOld = resolve;
              })
            : Promise.resolve("after-change"),
      });
      const off = observer.subscribe(() => {});
      client.setQueryData(["providers", "claude"], "unrelated");
      const refreshing =
        change === "profile"
          ? invalidateProfileCaches(client, "codex")
          : invalidateQueriesAfterChange(
              client,
              change === "sort" ? { queryKey: key } : {},
            );
      finishOld("before-change");
      await refreshing;
      expect(client.getQueryData(key)).toBe("after-change");
      expect(calls).toBe(2);
      if (change !== "restore") {
        expect(
          client.getQueryState(["providers", "claude"])?.isInvalidated,
        ).toBe(false);
      }
      off();
    },
  );
  it("invalidates inactive restored data without making eager requests", async () => {
    const client = new QueryClient();
    clients.push(client);
    client.setQueryData(["prompts"], "old");
    await invalidateQueriesAfterChange(client);
    expect(client.getQueryState(["prompts"])?.isInvalidated).toBe(true);
  });
});
