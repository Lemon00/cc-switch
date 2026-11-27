import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useXaiOauthQuotaByAccountId } from "@/lib/query/subscription";
import { subscriptionApi } from "@/lib/api/subscription";
vi.mock("@/lib/api/subscription", () => ({
  subscriptionApi: { getXaiOauthQuota: vi.fn() },
}));
let client: QueryClient;
const wrapper = ({ children }: PropsWithChildren) => (
  <QueryClientProvider client={client}>{children}</QueryClientProvider>
);
beforeEach(() => {
  vi.useFakeTimers();
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  vi.mocked(subscriptionApi.getXaiOauthQuota).mockReset().mockResolvedValue({
    tool: "grokbuild",
    credentialStatus: "valid",
    credentialMessage: null,
    success: true,
    tiers: [],
    extraUsage: null,
    error: null,
    queriedAt: 1,
  });
});
afterEach(() => {
  cleanup();
  client.clear();
  vi.useRealTimers();
});
describe("xAI quota polling contract", () => {
  it.each([0, 1, 7])("respects an interval of %i minutes", async (minutes) => {
    renderHook(
      () =>
        useXaiOauthQuotaByAccountId("one", {
          autoQuery: true,
          autoQueryIntervalMinutes: minutes,
        }),
      { wrapper },
    );
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(subscriptionApi.getXaiOauthQuota).toHaveBeenCalledTimes(1);
    if (minutes === 0) {
      await act(() => vi.advanceTimersByTimeAsync(20 * 60000));
      expect(subscriptionApi.getXaiOauthQuota).toHaveBeenCalledTimes(1);
    } else {
      await act(() => vi.advanceTimersByTimeAsync(minutes * 60000 - 1));
      expect(subscriptionApi.getXaiOauthQuota).toHaveBeenCalledTimes(1);
      await act(() => vi.advanceTimersByTimeAsync(1));
      expect(subscriptionApi.getXaiOauthQuota).toHaveBeenCalledTimes(2);
    }
  });
  it("does not poll when auto-query is disabled", async () => {
    renderHook(
      () =>
        useXaiOauthQuotaByAccountId("one", {
          autoQuery: false,
          autoQueryIntervalMinutes: 1,
        }),
      { wrapper },
    );
    await act(() => vi.advanceTimersByTimeAsync(20 * 60000));
    expect(subscriptionApi.getXaiOauthQuota).toHaveBeenCalledTimes(1);
  });
});
