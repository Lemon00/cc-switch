import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import {
  onlineManager,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppId } from "@/lib/api/types";
import {
  useAddProviderMutation,
  useUpdateProviderMutation,
  useDeleteProviderMutation,
  useSwitchProviderMutation,
  useSaveSettingsMutation,
} from "@/lib/query/mutations";
import { useAppMode, useSetProxyClientHost } from "@/lib/query/proxy";
import { useSettingsQuery } from "@/lib/query/queries";
import { queryClient as desktopClient } from "@/lib/query/queryClient";
import { usageKeys, isRemoteScriptUsageQueryKey } from "@/lib/query/usage";
const api = vi.hoisted(() => ({
  add: vi.fn(),
  update: vi.fn(),
  delete: vi.fn(),
  switch: vi.fn(),
  updateTrayMenu: vi.fn(),
  getAppMode: vi.fn(),
  setProxyClientHost: vi.fn(),
  get: vi.fn(),
  save: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  providersApi: api,
  settingsApi: api,
  sessionsApi: {},
}));
vi.mock("@/lib/api/proxy", () => ({ proxyApi: api }));
vi.mock("@/lib/toast", () => ({
  toast: { error: api.error, success: vi.fn() },
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (
      key: string,
      options?: {
        error?: string;
      },
    ) => options?.error ?? key,
  }),
}));
const clients: QueryClient[] = [];
function setup(
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  clients.push(client);
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}
const provider = { id: "same-id", name: "Fixture", settingsConfig: {} };
const mode = {
  mode: "route",
  attached: true,
  directProviderId: "claude-direct",
  routeProviderId: "claude-route",
};
beforeEach(() => {
  Object.values(api).forEach((fn) => fn.mockReset());
  api.updateTrayMenu.mockResolvedValue(true);
  onlineManager.setOnline(true);
});
afterEach(async () => {
  cleanup();
  onlineManager.setOnline(true);
  for (const client of clients.splice(0)) client.clear();
  await act(async () => {});
});
describe("desktop IPC query isolation", () => {
  it("reads local settings while the WebView reports offline", async () => {
    onlineManager.setOnline(false);
    api.get.mockResolvedValue({ showInTray: true });
    const { wrapper } = setup(desktopClient);
    const { result } = renderHook(() => useSettingsQuery(), { wrapper });
    await act(async () => {});
    expect(api.get).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(result.current.data).toEqual({ showInTray: true }),
    );
  });
  it("writes local settings immediately while offline", async () => {
    onlineManager.setOnline(false);
    api.save.mockResolvedValue(true);
    const { wrapper } = setup(desktopClient);
    const { result } = renderHook(() => useSaveSettingsMutation(), { wrapper });
    await act(async () => {
      result.current.mutate({ showInTray: false });
    });
    expect(api.save).toHaveBeenCalledWith({ showInTray: false });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });
  it("does not display another application's mode during a cold switch", async () => {
    api.getAppMode
      .mockResolvedValueOnce(mode)
      .mockImplementation(() => new Promise(() => {}));
    const { wrapper } = setup();
    const { result, rerender } = renderHook(({ app }) => useAppMode(app), {
      wrapper,
      initialProps: { app: "claude" },
    });
    await waitFor(() => expect(result.current.data).toEqual(mode));
    rerender({ app: "codex" });
    expect(result.current.data).toBeUndefined();
    rerender({ app: "claude" });
    expect(result.current.data).toEqual(mode);
  });
  const operations: Array<[string, (app: AppId) => () => Promise<unknown>]> = [
    [
      "add",
      (app) => {
        const mutation = useAddProviderMutation(app);
        return () => mutation.mutateAsync(provider);
      },
    ],
    [
      "update",
      (app) => {
        const mutation = useUpdateProviderMutation(app);
        return () => mutation.mutateAsync({ provider });
      },
    ],
    [
      "delete",
      (app) => {
        const mutation = useDeleteProviderMutation(app);
        return () => mutation.mutateAsync(provider.id);
      },
    ],
    [
      "switch",
      (app) => {
        const mutation = useSwitchProviderMutation(app);
        return () => mutation.mutateAsync(provider.id);
      },
    ],
  ];
  it.each(operations)(
    "keeps %s completion attached to its original app",
    async (name, useOperation) => {
      let finish!: (value: unknown) => void;
      api[name as "add"].mockImplementation(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
      const { client, wrapper } = setup();
      client.setQueryData(["providers", "claude"], {});
      client.setQueryData(["providers", "codex"], {});
      const { result, rerender } = renderHook(
        ({ app }: { app: AppId }) => useOperation(app),
        { wrapper, initialProps: { app: "claude" as AppId } },
      );
      let pending!: Promise<unknown>;
      await act(async () => {
        pending = result.current();
      });
      rerender({ app: "codex" });
      await act(async () => {
        finish(name === "switch" ? { warnings: [] } : true);
        await pending;
      });
      expect(client.getQueryState(["providers", "claude"])?.isInvalidated).toBe(
        true,
      );
      expect(client.getQueryState(["providers", "codex"])?.isInvalidated).toBe(
        false,
      );
    },
  );
  it("does not redirect a paused write to the newly selected application", async () => {
    onlineManager.setOnline(false);
    api.update.mockResolvedValue(true);
    const { wrapper } = setup();
    const { result, rerender } = renderHook(
      ({ app }: { app: AppId }) => useUpdateProviderMutation(app),
      { wrapper, initialProps: { app: "claude" as AppId } },
    );
    let pending!: Promise<unknown>;
    await act(async () => {
      pending = result.current.mutateAsync({ provider });
    });
    await waitFor(() => expect(result.current.isPaused).toBe(true));
    rerender({ app: "codex" });
    await act(async () => {
      onlineManager.setOnline(true);
      await pending;
    });
    expect(api.update).toHaveBeenCalledWith(
      provider,
      "claude",
      undefined,
      undefined,
    );
  });
  it("keeps a string IPC error visible", async () => {
    api.setProxyClientHost.mockRejectedValue(
      "native configuration is read-only",
    );
    const { wrapper } = setup();
    const { result } = renderHook(() => useSetProxyClientHost(), { wrapper });
    await act(async () => {
      await expect(result.current.mutateAsync("localhost")).rejects.toBe(
        "native configuration is read-only",
      );
    });
    expect(api.error).toHaveBeenCalledWith("native configuration is read-only");
  });
  it("gives remote usage its own namespace even for reserved provider IDs", () => {
    expect(usageKeys.script("detail", "claude")).not.toEqual(
      usageKeys.detail("claude"),
    );
    for (const id of ["detail", "summary", "pricing", "script", "ordinary"]) {
      expect(isRemoteScriptUsageQueryKey(usageKeys.script(id, "claude"))).toBe(
        true,
      );
    }
    expect(isRemoteScriptUsageQueryKey(usageKeys.detail("claude"))).toBe(false);
  });
});
