import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import {
  QueryClient,
  QueryClientProvider,
  QueryObserver,
  type QueryKey,
} from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  useDeleteSessionMutation,
  useSaveSettingsMutation,
} from "@/lib/query/mutations";
import {
  useClearProfileMutation,
  useCreateProfileMutation,
  useDeleteProfileMutation,
  useUpdateProfileMutation,
} from "@/lib/query/profiles";
import {
  useAddCustomIconMutation,
  useDeleteCustomIconMutation,
  useRenameCustomIconMutation,
} from "@/lib/query/customIcons";
import {
  useDisableCurrentOmo,
  useDisableCurrentOmoSlim,
} from "@/lib/query/omo";
import {
  useAdoptCodexStackCatalog,
  useSetProxyClientHost,
  useSetProxyStackMember,
  useSetProxyTakeoverForApp,
  useUpdateAppProxyConfig,
  useUpdateGlobalProxyConfig,
} from "@/lib/query/proxy";
import {
  useAddToFailoverQueue,
  useAutoFailoverEnabled,
  useRemoveFromFailoverQueue,
  useResetCircuitBreaker,
  useSetAutoFailoverEnabled,
  useUpdateCircuitBreakerConfig,
} from "@/lib/query/failover";
import { useDeleteModelPricing, usageKeys } from "@/lib/query/usage";
import { useSetGlobalProxyUrl } from "@/hooks/useGlobalProxy";
import { useProviderActions } from "@/hooks/useProviderActions";
import type { AppProxyConfig, GlobalProxyConfig } from "@/types/proxy";
const ipc = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: ipc,
}));
vi.mock("@/lib/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));
const clients: QueryClient[] = [];
const subscriptions: Array<() => void> = [];
beforeEach(() => {
  ipc.mockReset().mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  subscriptions.splice(0).forEach((off) => off());
  clients.splice(0).forEach((client) => client.clear());
});
function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  clients.push(client);
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}
function coldRead(client: QueryClient, key: QueryKey) {
  let finish!: (value: string) => void;
  const read = vi
    .fn()
    .mockImplementationOnce(
      () => new Promise<string>((resolve) => (finish = resolve)),
    )
    .mockResolvedValue("after-write");
  const observer = new QueryObserver(client, { queryKey: key, queryFn: read });
  subscriptions.push(observer.subscribe(() => {}));
  return { read, finish: () => finish("before-write") };
}
function operation<T>(
  useMutation: () => {
    mutateAsync: (variables: T) => Promise<unknown>;
  },
  variables: T,
) {
  return () => {
    const mutation = useMutation();
    return () => mutation.mutateAsync(variables);
  };
}
const member = { appType: "claude", providerId: "one", enabled: true };
const circuitConfig = {
  failureThreshold: 5,
  successThreshold: 2,
  timeoutSeconds: 60,
  errorRateThreshold: 0.5,
  minRequests: 10,
};
const cases: Array<{
  name: string;
  useOperation: () => () => Promise<unknown>;
  keys: QueryKey[];
}> = [
  ...(["claude"] as const).map((app) => ({
    name: `${app} usage script save`,
    useOperation: () => {
      ipc.mockResolvedValue(true);
      const actions = useProviderActions(app);
      return () =>
        actions.saveUsageScript(
          { id: "one", name: "Provider", settingsConfig: {} },
          { enabled: true, language: "javascript", code: "return {};" },
        );
    },
    keys: [
      ["providers", app],
      usageKeys.script("one", app),
      ["subscription", "quota", app],
    ],
  })),
  {
    name: "OpenCode default model save",
    useOperation: () => {
      const actions = useProviderActions("opencode");
      return () =>
        actions.setAsDefaultModel(
          {
            id: "one",
            name: "Provider",
            settingsConfig: { models: { model: { name: "Model" } } },
          },
          "model",
        );
    },
    keys: [["opencodeDefaultModel"]],
  },
  {
    name: "outbound proxy save",
    useOperation: operation(useSetGlobalProxyUrl, "http://127.0.0.1:8080"),
    keys: [["globalProxyUrl"], ["upstreamProxyStatus"]],
  },
  {
    name: "settings save",
    useOperation: operation(useSaveSettingsMutation, { showInTray: false }),
    keys: [["settings"], ["opencode", "runtime-models"]],
  },
  {
    name: "profile creation",
    useOperation: operation(useCreateProfileMutation, {
      name: "new",
      scope: "codex",
    }),
    keys: [["profiles"]],
  },
  {
    name: "profile update",
    useOperation: operation(useUpdateProfileMutation, {
      id: "one",
      name: "renamed",
    }),
    keys: [["profiles"]],
  },
  {
    name: "profile deletion",
    useOperation: operation(useDeleteProfileMutation, "one"),
    keys: [["profiles"]],
  },
  {
    name: "profile clearing",
    useOperation: operation(useClearProfileMutation, "codex"),
    keys: [["profiles"]],
  },
  {
    name: "icon creation",
    useOperation: operation(useAddCustomIconMutation, {
      name: "new",
      data: "data:image/png;base64,AA==",
    }),
    keys: [["customIcons"]],
  },
  {
    name: "icon rename",
    useOperation: operation(useRenameCustomIconMutation, {
      id: "one",
      name: "new",
    }),
    keys: [["customIcons"]],
  },
  {
    name: "icon deletion",
    useOperation: operation(useDeleteCustomIconMutation, "one"),
    keys: [["customIcons"]],
  },
  {
    name: "OMO disable",
    useOperation: operation(useDisableCurrentOmo, undefined),
    keys: [
      ["providers", "opencode"],
      ["omo", "current-provider-id"],
    ],
  },
  {
    name: "OMO Slim disable",
    useOperation: operation(useDisableCurrentOmoSlim, undefined),
    keys: [
      ["providers", "opencode"],
      ["omo-slim", "current-provider-id"],
    ],
  },
  {
    name: "stack membership",
    useOperation: operation(useSetProxyStackMember, member),
    keys: [["providers", "claude", "stack"]],
  },
  {
    name: "stack catalog adoption",
    useOperation: operation(useAdoptCodexStackCatalog, undefined),
    keys: [["providers", "codex", "stack"]],
  },
  {
    name: "proxy host save",
    useOperation: operation(useSetProxyClientHost, "localhost"),
    keys: [["proxyClientHost"]],
  },
  {
    name: "takeover",
    useOperation: operation(useSetProxyTakeoverForApp, member),
    keys: [
      ["proxyStatus"],
      ["proxyTakeoverStatus"],
      ["providers", "claude", "mode"],
    ],
  },
  {
    name: "global proxy save",
    useOperation: operation(
      useUpdateGlobalProxyConfig,
      {} as GlobalProxyConfig,
    ),
    keys: [["globalProxyConfig"], ["proxyStatus"]],
  },
  {
    name: "app proxy save",
    useOperation: operation(useUpdateAppProxyConfig, {
      appType: "claude",
    } as AppProxyConfig),
    keys: [
      ["appProxyConfig", "claude"],
      ["autoFailoverEnabled", "claude"],
      ["circuitBreakerConfig"],
      ["proxyStatus"],
    ],
  },
  {
    name: "circuit reset",
    useOperation: operation(useResetCircuitBreaker, member),
    keys: [
      ["providerHealth", "one", "claude"],
      ["circuitBreakerStats", "one", "claude"],
      ["providers", "claude"],
      ["proxyStatus"],
    ],
  },
  {
    name: "circuit config save",
    useOperation: operation(useUpdateCircuitBreakerConfig, circuitConfig),
    keys: [
      ["circuitBreakerConfig"],
      ["appProxyConfig", "claude"],
      ["appProxyConfig", "codex"],
    ],
  },
  {
    name: "failover queue add",
    useOperation: operation(useAddToFailoverQueue, member),
    keys: [
      ["failoverQueue", "claude"],
      ["availableProvidersForFailover", "claude"],
      ["providers", "claude"],
    ],
  },
  {
    name: "failover queue remove",
    useOperation: operation(useRemoveFromFailoverQueue, member),
    keys: [
      ["failoverQueue", "claude"],
      ["providerHealth", "one", "claude"],
      ["circuitBreakerStats", "one", "claude"],
    ],
  },
  {
    name: "failover toggle",
    useOperation: operation(useSetAutoFailoverEnabled, member),
    keys: [
      ["appProxyConfig", "claude"],
      ["failoverQueue", "claude"],
      ["availableProvidersForFailover", "claude"],
      ["providers", "claude"],
      ["proxyStatus"],
    ],
  },
  {
    name: "pricing deletion",
    useOperation: operation(useDeleteModelPricing, "one"),
    keys: [usageKeys.pricing(), usageKeys.detail("one")],
  },
];
describe("persisted writes racing the initial query", () => {
  it("reads back failover state after a cold optimistic toggle fails", async () => {
    let finishRead!: (value: boolean) => void;
    let rejectWrite!: (error: Error) => void;
    let reads = 0;
    ipc.mockImplementation((command) => {
      if (command === "get_auto_failover_enabled") {
        if (++reads === 1)
          return new Promise((resolve) => (finishRead = resolve));
        return Promise.resolve(false);
      }
      if (command === "set_auto_failover_enabled") {
        return new Promise((_, reject) => (rejectWrite = reject));
      }
      return Promise.resolve(undefined);
    });
    const { client, wrapper } = setup();
    const { result: state } = renderHook(
      () => useAutoFailoverEnabled("claude"),
      { wrapper },
    );
    const { result } = renderHook(() => useSetAutoFailoverEnabled(), {
      wrapper,
    });
    let pending!: Promise<unknown>;
    await act(async () => {
      pending = result.current.mutateAsync(member).catch((error) => error);
    });
    await waitFor(() => expect(state.current.data).toBe(true));
    await act(async () => {
      rejectWrite(new Error("write failed"));
      await pending;
      finishRead(true);
    });
    await waitFor(() => expect(state.current.data).toBe(false));
    expect(reads).toBe(2);
    expect(client.getQueryData(["autoFailoverEnabled", "claude"])).toBe(false);
  });

  it.each(cases)(
    "$name discards all pre-write snapshots",
    async ({ useOperation, keys }) => {
      const { client, wrapper } = setup();
      const reads = keys.map((key) => coldRead(client, key));
      const unrelated = ["providers", "unrelated"];
      client.setQueryData(unrelated, "unrelated");
      const { result } = renderHook(useOperation, { wrapper });
      let pending!: Promise<unknown>;
      await act(async () => {
        pending = result.current();
      });
      await act(async () => {
        reads.forEach(({ finish }) => finish());
        await pending;
      });
      keys.forEach((key) =>
        expect(client.getQueryData(key), JSON.stringify(key)).toBe(
          "after-write",
        ),
      );
      reads.forEach(({ read }) => expect(read).toHaveBeenCalledTimes(2));
      expect(client.getQueryState(unrelated)?.isInvalidated).toBe(false);
    },
  );
  it("reconciles a persisted proxy host even if rewriting the client fails", async () => {
    ipc.mockRejectedValue("client is read-only");
    const { client, wrapper } = setup();
    const key = ["proxyClientHost"];
    const { finish } = coldRead(client, key);
    const { result } = renderHook(() => useSetProxyClientHost(), { wrapper });
    let failure!: Promise<unknown>;
    await act(async () => {
      failure = result.current.mutateAsync("localhost").catch((error) => error);
    });
    await act(async () => {
      finish();
      expect(await failure).toBe("client is read-only");
    });
    expect(client.getQueryData(key)).toBe("after-write");
  });
  it("does not resurrect a deleted session when an inactive list read finishes", async () => {
    ipc.mockResolvedValue(true);
    const { client, wrapper } = setup();
    const deleted = {
      providerId: "claude",
      sessionId: "one",
      sourcePath: "/one",
    };
    const retained = { ...deleted, sessionId: "two", sourcePath: "/two" };
    client.setQueryData(["sessions"], [deleted, retained]);
    let finish!: (value: (typeof deleted)[]) => void;
    const oldRead = client
      .fetchQuery({
        queryKey: ["sessions"],
        staleTime: 0,
        queryFn: () =>
          new Promise<(typeof deleted)[]>((resolve) => {
            finish = resolve;
          }),
      })
      .catch(() => undefined);
    const { result } = renderHook(() => useDeleteSessionMutation(), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync(deleted);
    });
    await act(async () => {
      finish([deleted, retained]);
      await oldRead;
    });
    expect(client.getQueryData(["sessions"])).toEqual([retained]);
    expect(client.getQueryState(["sessions"])?.isInvalidated).toBe(true);
  });
});
