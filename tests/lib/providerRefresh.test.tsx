import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import {
  focusManager,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  refreshNativeProviderCatalog,
  refreshProviderLiveState,
  settleProviderChange,
} from "@/lib/query/providerActions";
import { providersQueryOptions, useProvidersQuery } from "@/lib/query/queries";
const api = vi.hoisted(() => ({
  refresh: vi.fn(),
  getAll: vi.fn(),
  getCurrent: vi.fn(),
}));
vi.mock("@/lib/api", () => ({ providersApi: api }));
const clients: QueryClient[] = [];
function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  clients.push(client);
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
beforeEach(() => {
  api.refresh.mockReset().mockResolvedValue({});
  api.getAll.mockReset().mockResolvedValue({});
  api.getCurrent.mockReset().mockResolvedValue("");
  focusManager.setFocused(true);
});
afterEach(() => {
  cleanup();
  clients.splice(0).forEach((client) => client.clear());
  focusManager.setFocused(undefined);
  vi.useRealTimers();
  vi.restoreAllMocks();
});
describe("native provider refresh", () => {
  it("prefetches only a saved snapshot", async () => {
    const { client } = setup();
    await client.prefetchQuery(providersQueryOptions("dsh"));
    expect(api.refresh).not.toHaveBeenCalled();
    expect(api.getAll).toHaveBeenCalledWith("dsh");
  });
  it("deduplicates concurrent observers but isolates clients and apps", async () => {
    const pending = deferred<Record<string, never>>();
    api.refresh.mockReturnValue(pending.promise);
    const first = setup().client;
    const second = setup().client;
    const one = refreshNativeProviderCatalog(first, "dsh");
    expect(refreshNativeProviderCatalog(first, "dsh")).toBe(one);
    const three = refreshNativeProviderCatalog(second, "dsh");
    expect(api.refresh).toHaveBeenCalledTimes(2);
    pending.resolve({});
    await Promise.all([one, three]);
  });
  it("reconciles again when a mutation settles during an older refresh", async () => {
    const old = deferred<Record<string, never>>();
    api.refresh.mockReturnValueOnce(old.promise);
    const { client } = setup();
    const background = refreshNativeProviderCatalog(client, "dsh");
    const written = settleProviderChange(client, "dsh");
    old.resolve({});
    await Promise.all([background, written]);
    expect(api.refresh).toHaveBeenCalledTimes(2);
  });
  it("does not hide explicit refresh errors and allows a later retry", async () => {
    api.refresh.mockRejectedValueOnce("unreadable native file");
    const { client } = setup();
    client.setQueryData(["providers", "dsh"], {});
    await expect(refreshProviderLiveState(client, "dsh")).rejects.toBe(
      "unreadable native file",
    );
    expect(client.getQueryState(["providers", "dsh"])?.isInvalidated).toBe(
      true,
    );
    await refreshProviderLiveState(client, "dsh");
    expect(api.refresh).toHaveBeenCalledTimes(2);
  });
  it("coalesces writes waiting on a pass and recovers from an older failed pass", async () => {
    const old = deferred<Record<string, never>>();
    api.refresh.mockReturnValueOnce(old.promise);
    const { client } = setup();
    const background = refreshNativeProviderCatalog(client, "dsh");
    const writes = [
      settleProviderChange(client, "dsh"),
      settleProviderChange(client, "dsh"),
    ];
    old.reject("old native read failed");
    await Promise.all([background, ...writes]);
    expect(api.refresh).toHaveBeenCalledTimes(2);
  });
  it("keeps post-mutation recovery best effort", async () => {
    api.refresh.mockRejectedValue("native read failed");
    const log = vi.spyOn(console, "warn").mockImplementation(() => {});
    await expect(
      settleProviderChange(setup().client, "dsh"),
    ).resolves.toBeUndefined();
    expect(log).toHaveBeenCalledWith(
      expect.stringContaining("dsh"),
      "native read failed",
    );
  });
  it("pauses native polling while hidden and reconciles when native visibility returns", async () => {
    vi.useFakeTimers();
    const { wrapper } = setup();
    const { unmount } = renderHook(
      () => useProvidersQuery("dsh", { isProxyRunning: true }),
      { wrapper },
    );
    await act(async () => {});
    api.refresh.mockClear();
    act(() => focusManager.setFocused(false));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30000);
    });
    expect(api.refresh).not.toHaveBeenCalled();
    await act(async () => {
      focusManager.setFocused(true);
    });
    expect(api.refresh).toHaveBeenCalledTimes(1);
    unmount();
    api.refresh.mockClear();
    await act(async () => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
      await vi.advanceTimersByTimeAsync(30000);
    });
    expect(api.refresh).not.toHaveBeenCalled();
  });
});
