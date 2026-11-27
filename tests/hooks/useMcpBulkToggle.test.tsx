import type { PropsWithChildren } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useBulkToggleMcpApp,
  useDeleteMcpServer,
  useImportMcpFromApps,
  useToggleMcpApp,
  useUpsertMcpServer,
  useAllMcpServers,
} from "@/hooks/useMcp";
import type { McpServer } from "@/types";
const toggleAppMock = vi.hoisted(() => vi.fn());
const upsertServerMock = vi.hoisted(() => vi.fn());
const deleteServerMock = vi.hoisted(() => vi.fn());
const importFromAppsMock = vi.hoisted(() => vi.fn());
const getAllServersMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api/mcp", () => ({
  mcpApi: {
    getAllServers: getAllServersMock,
    toggleApp: toggleAppMock,
    upsertUnifiedServer: upsertServerMock,
    deleteUnifiedServer: deleteServerMock,
    importFromApps: importFromAppsMock,
  },
}));
function createWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: PropsWithChildren) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}
describe("MCP management mutation hooks", () => {
  beforeEach(() => {
    toggleAppMock.mockReset();
    upsertServerMock.mockReset();
    deleteServerMock.mockReset();
    importFromAppsMock.mockReset();
    getAllServersMock.mockReset();
  });
  it("imports during the initial list read discard the stale response", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const wrapper = createWrapper(queryClient);
    let resolveOld!: (value: Record<string, McpServer>) => void;
    const fresh = { imported: { id: "imported" } as McpServer };
    getAllServersMock
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
      )
      .mockResolvedValue(fresh);
    importFromAppsMock.mockResolvedValue(undefined);
    const list = renderHook(() => useAllMcpServers(), { wrapper });
    const mutation = renderHook(() => useImportMcpFromApps(), { wrapper });
    await waitFor(() => expect(getAllServersMock).toHaveBeenCalledTimes(1));
    let importing!: Promise<unknown>;
    act(() => {
      importing = mutation.result.current.mutateAsync();
    });
    await waitFor(() => expect(getAllServersMock).toHaveBeenCalledTimes(2));
    await act(async () => {
      await importing;
      resolveOld({});
    });
    expect(queryClient.getQueryData(["mcp", "all"])).toEqual(fresh);
    list.unmount();
    mutation.unmount();
    queryClient.clear();
  });
  it("rolling back one toggle preserves another completed server toggle", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const wrapper = createWrapper(queryClient);
    queryClient.setQueryData(["mcp", "all"], {
      alpha: { id: "alpha", apps: { claude: false } },
      beta: { id: "beta", apps: { codex: false } },
    });
    let rejectFirst!: (error: Error) => void;
    toggleAppMock
      .mockReturnValueOnce(
        new Promise((_, reject) => {
          rejectFirst = reject;
        }),
      )
      .mockResolvedValue(undefined);
    const first = renderHook(() => useToggleMcpApp(), { wrapper });
    const second = renderHook(() => useToggleMcpApp(), { wrapper });
    let pending!: Promise<unknown>;
    act(() => {
      pending = first.result.current
        .mutateAsync({ serverId: "alpha", app: "claude", enabled: true })
        .catch((error) => error);
    });
    await waitFor(() => expect(toggleAppMock).toHaveBeenCalledTimes(1));
    await act(async () => {
      await second.result.current.mutateAsync({
        serverId: "beta",
        app: "codex",
        enabled: true,
      });
    });
    await act(async () => {
      rejectFirst(new Error("write failed"));
      await pending;
    });
    expect(queryClient.getQueryData(["mcp", "all"])).toEqual({
      alpha: { id: "alpha", apps: { claude: false } },
      beta: { id: "beta", apps: { codex: true } },
    });
    first.unmount();
    second.unmount();
    queryClient.clear();
  });
  it("runs bulk writes serially and invalidates the list once", async () => {
    let releaseFirst: (() => void) | undefined;
    let releaseInvalidation: (() => void) | undefined;
    const firstPending = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const invalidationPending = new Promise<void>((resolve) => {
      releaseInvalidation = resolve;
    });
    toggleAppMock.mockImplementation(async (serverId: string) => {
      if (serverId === "alpha") await firstPending;
    });
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi
      .spyOn(queryClient, "invalidateQueries")
      .mockImplementation(() => invalidationPending);
    const { result } = renderHook(() => useBulkToggleMcpApp(), {
      wrapper: createWrapper(queryClient),
    });
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.mutateAsync({
        serverIds: ["alpha", "beta"],
        app: "claude",
        enabled: true,
      });
    });
    await waitFor(() => expect(toggleAppMock).toHaveBeenCalledTimes(1));
    releaseFirst?.();
    await waitFor(() => expect(toggleAppMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1));
    expect(result.current.isPending).toBe(true);
    releaseInvalidation?.();
    await act(async () => {
      await mutation;
    });
    expect(toggleAppMock.mock.calls).toEqual([
      ["alpha", "claude", true],
      ["beta", "claude", true],
    ]);
    expect(invalidateSpy).toHaveBeenCalledTimes(1);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["mcp", "all"] });
    await waitFor(() => expect(result.current.isPending).toBe(false));
  });
  it("refreshes the list when a single live-config write fails", async () => {
    toggleAppMock.mockRejectedValueOnce(new Error("write failed"));
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { result } = renderHook(() => useToggleMcpApp(), {
      wrapper: createWrapper(queryClient),
    });
    await act(async () => {
      await expect(
        result.current.mutateAsync({
          serverId: "alpha",
          app: "claude",
          enabled: true,
        }),
      ).rejects.toThrow("write failed");
    });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["mcp", "all"] });
  });
  it("keeps a single toggle pending until the refreshed list is available", async () => {
    let releaseInvalidation: (() => void) | undefined;
    const invalidationPending = new Promise<void>((resolve) => {
      releaseInvalidation = resolve;
    });
    toggleAppMock.mockResolvedValueOnce(undefined);
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi
      .spyOn(queryClient, "invalidateQueries")
      .mockImplementation(() => invalidationPending);
    const { result } = renderHook(() => useToggleMcpApp(), {
      wrapper: createWrapper(queryClient),
    });
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.mutateAsync({
        serverId: "alpha",
        app: "claude",
        enabled: true,
      });
    });
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1));
    expect(result.current.isPending).toBe(true);
    releaseInvalidation?.();
    await act(async () => {
      await mutation;
    });
    await waitFor(() => expect(result.current.isPending).toBe(false));
  });
  it("refreshes and stays pending when an upsert fails after persistence", async () => {
    let releaseInvalidation: (() => void) | undefined;
    const invalidationPending = new Promise<void>((resolve) => {
      releaseInvalidation = resolve;
    });
    upsertServerMock.mockRejectedValueOnce(new Error("live sync failed"));
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi
      .spyOn(queryClient, "invalidateQueries")
      .mockImplementation(() => invalidationPending);
    const { result } = renderHook(() => useUpsertMcpServer(), {
      wrapper: createWrapper(queryClient),
    });
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.mutateAsync({ id: "alpha" } as McpServer);
    });
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1));
    expect(result.current.isPending).toBe(true);
    releaseInvalidation?.();
    await act(async () => {
      await expect(mutation).rejects.toThrow("live sync failed");
    });
    await waitFor(() => expect(result.current.isPending).toBe(false));
  });
  it("refreshes and stays pending when deletion fails after persistence", async () => {
    let releaseInvalidation: (() => void) | undefined;
    const invalidationPending = new Promise<void>((resolve) => {
      releaseInvalidation = resolve;
    });
    deleteServerMock.mockRejectedValueOnce(new Error("live cleanup failed"));
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi
      .spyOn(queryClient, "invalidateQueries")
      .mockImplementation(() => invalidationPending);
    const { result } = renderHook(() => useDeleteMcpServer(), {
      wrapper: createWrapper(queryClient),
    });
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.mutateAsync("alpha");
    });
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1));
    expect(result.current.isPending).toBe(true);
    releaseInvalidation?.();
    await act(async () => {
      await expect(mutation).rejects.toThrow("live cleanup failed");
    });
    await waitFor(() => expect(result.current.isPending).toBe(false));
  });
  it("keeps an import pending until the complete server list is refreshed", async () => {
    let releaseInvalidation: (() => void) | undefined;
    const invalidationPending = new Promise<void>((resolve) => {
      releaseInvalidation = resolve;
    });
    importFromAppsMock.mockResolvedValueOnce(2);
    const queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    const invalidateSpy = vi
      .spyOn(queryClient, "invalidateQueries")
      .mockImplementation(() => invalidationPending);
    const { result } = renderHook(() => useImportMcpFromApps(), {
      wrapper: createWrapper(queryClient),
    });
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.mutateAsync();
    });
    await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1));
    expect(result.current.isPending).toBe(true);
    releaseInvalidation?.();
    await act(async () => {
      await mutation;
    });
    await waitFor(() => expect(result.current.isPending).toBe(false));
  });
});
