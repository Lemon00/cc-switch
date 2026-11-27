import type { PropsWithChildren } from "react";
import { act, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, it } from "vitest";
import { useUsageCacheBridge } from "@/hooks/useUsageCacheBridge";
import { usageKeys } from "@/lib/query/usage";
import { emitTauriEvent } from "../msw/tauriMocks";
import { subscriptionKeys } from "@/lib/query/subscription";
it("keeps backend event data when an older IPC query resolves later", async () => {
  const client = new QueryClient();
  const queryKey = usageKeys.script("one", "claude");
  let finish!: (value: unknown) => void;
  const pending = client
    .fetchQuery({
      queryKey,
      queryFn: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    })
    .catch(() => {});
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const { unmount } = renderHook(() => useUsageCacheBridge(), { wrapper });
  const newest = { success: true, data: [{ remaining: 8 }] };
  await act(async () => {
    emitTauriEvent("usage-cache-updated", {
      kind: "script",
      appType: "claude",
      providerId: "one",
      data: newest,
    });
  });
  expect(client.getQueryData(queryKey)).toEqual(newest);
  await act(async () => {
    finish({ success: true, data: [{ remaining: 99 }] });
    await pending;
  });
  expect(client.getQueryData(queryKey)).toEqual(newest);
  unmount();
  client.clear();
});
it("keeps consecutive events in order and isolates script/subscription scopes", async () => {
  const client = new QueryClient();
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const { unmount } = renderHook(() => useUsageCacheBridge(), { wrapper });
  await act(async () => {
    emitTauriEvent("usage-cache-updated", {
      kind: "script",
      appType: "claude",
      providerId: "one",
      data: { success: true, data: [{ remaining: 9 }] },
    });
    emitTauriEvent("usage-cache-updated", {
      kind: "subscription",
      appType: "codex",
      data: { success: false, error: "expired" },
    });
    emitTauriEvent("usage-cache-updated", {
      kind: "script",
      appType: "claude",
      providerId: "one",
      data: { success: true, data: [{ remaining: 7 }] },
    });
  });
  expect(client.getQueryData(usageKeys.script("one", "claude"))).toEqual({
    success: true,
    data: [{ remaining: 7 }],
  });
  expect(client.getQueryData(subscriptionKeys.quota("codex"))).toEqual({
    success: false,
    error: "expired",
  });
  expect(client.getQueryData(usageKeys.script("one", "codex"))).toBeUndefined();
  unmount();
  client.clear();
});
it("keeps the latest event when the first event cancels an in-flight read", async () => {
  const client = new QueryClient();
  const queryKey = usageKeys.script("one", "claude");
  let finish!: (value: unknown) => void;
  const pending = client
    .fetchQuery({
      queryKey,
      queryFn: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    })
    .catch(() => {});
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const { unmount } = renderHook(() => useUsageCacheBridge(), { wrapper });
  await act(async () => {
    for (const remaining of [9, 7]) {
      emitTauriEvent("usage-cache-updated", {
        kind: "script",
        appType: "claude",
        providerId: "one",
        data: { success: true, data: [{ remaining }] },
      });
    }
  });
  expect(client.getQueryData(queryKey)).toEqual({
    success: true,
    data: [{ remaining: 7 }],
  });
  finish({ success: true, data: [{ remaining: 99 }] });
  await pending;
  unmount();
  client.clear();
});
