import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import {
  focusManager,
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
import { useUsageEventBridge } from "@/hooks/useUsageEventBridge";
import { localUsageQueryFilters, usageKeys } from "@/lib/query/usage";
import { emitTauriEvent } from "../msw/tauriMocks";
describe("useUsageEventBridge", () => {
  beforeEach(() => focusManager.setFocused(true));
  afterEach(() => {
    cleanup();
    focusManager.setFocused(undefined);
  });
  it("refreshes usage queries on every backend event until unmount", async () => {
    const client = new QueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const wrapper = ({ children }: PropsWithChildren) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { unmount } = renderHook(() => useUsageEventBridge(), { wrapper });
    await act(async () => {});
    await act(async () => {
      emitTauriEvent("usage-log-recorded", null);
      emitTauriEvent("usage-log-recorded", null);
    });
    expect(invalidate).toHaveBeenCalledTimes(2);
    expect(invalidate).toHaveBeenCalledWith({
      ...localUsageQueryFilters,
      refetchType: "active",
    });
    unmount();
    act(() => emitTauriEvent("usage-log-recorded", null));
    expect(invalidate).toHaveBeenCalledTimes(2);
    client.clear();
  });
  it("keeps background data stale and fetches once on return", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const query = vi.fn().mockResolvedValue(1);
    const wrapper = ({ children }: PropsWithChildren) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result, unmount } = renderHook(
      () => {
        useUsageEventBridge();
        return useQuery({
          queryKey: [...usageKeys.all, "summary"],
          queryFn: query,
          staleTime: Infinity,
        });
      },
      { wrapper },
    );
    await waitFor(() => expect(result.current.data).toBe(1));
    act(() => focusManager.setFocused(false));
    query.mockResolvedValue(2);
    await act(async () => {
      for (let i = 0; i < 10; i++) emitTauriEvent("usage-log-recorded", null);
    });
    expect(query).toHaveBeenCalledTimes(1);
    expect(
      client.getQueryState([...usageKeys.all, "summary"])?.isInvalidated,
    ).toBe(true);
    act(() => focusManager.setFocused(true));
    await waitFor(() => expect(result.current.data).toBe(2));
    expect(query).toHaveBeenCalledTimes(2);
    unmount();
    client.clear();
  });
  it("does not invalidate or refetch remote balances on local log events", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const remote = vi.fn().mockResolvedValue({ success: true });
    const local = vi.fn().mockResolvedValue(1);
    const wrapper = ({ children }: PropsWithChildren) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { unmount } = renderHook(
      () => {
        useUsageEventBridge();
        useQuery({
          queryKey: usageKeys.script("detail", "claude"),
          queryFn: remote,
          staleTime: Infinity,
        });
        useQuery({
          queryKey: usageKeys.detail("claude"),
          queryFn: local,
          staleTime: Infinity,
        });
      },
      { wrapper },
    );
    await waitFor(() => expect(remote).toHaveBeenCalledTimes(1));
    await act(async () => {
      emitTauriEvent("usage-log-recorded", null);
    });
    expect(remote).toHaveBeenCalledTimes(1);
    expect(local).toHaveBeenCalledTimes(2);
    expect(
      client.getQueryState(usageKeys.script("detail", "claude"))?.isInvalidated,
    ).toBe(false);
    unmount();
    client.clear();
  });
  it.each([true, false])(
    "discards a pre-event initial read while focused=%s",
    async (focused) => {
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const key = [...usageKeys.all, "summary"];
      let finish!: (value: number) => void;
      const query = vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<number>((resolve) => {
              finish = resolve;
            }),
        )
        .mockResolvedValue(2);
      const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      );
      const { result, unmount } = renderHook(
        () => {
          useUsageEventBridge();
          return useQuery({
            queryKey: key,
            queryFn: query,
            staleTime: Infinity,
          });
        },
        { wrapper },
      );
      await act(async () => {});
      act(() => focusManager.setFocused(focused));
      await act(async () => {
        emitTauriEvent("usage-log-recorded", null);
        finish(1);
      });
      if (!focused) {
        expect(client.getQueryData(key)).toBeUndefined();
        expect(client.getQueryState(key)?.isInvalidated).toBe(true);
        expect(query).toHaveBeenCalledTimes(1);
        act(() => focusManager.setFocused(true));
      }
      await waitFor(() => expect(result.current.data).toBe(2));
      expect(query).toHaveBeenCalledTimes(2);
      unmount();
      client.clear();
    },
  );
});
