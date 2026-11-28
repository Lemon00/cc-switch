import type { ReactNode } from "react";
import { renderHook, act, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { useQuery } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { proxyKeys } from "@/lib/query/proxy";
import { createTestQueryClient } from "../utils/testQueryClient";
const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();
const invokeMock = vi.fn();
vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (key === "proxy.server.started") {
        return `代理服务已启动 - ${options?.address}:${options?.port}`;
      }
      if (typeof options?.defaultValue === "string") {
        return options.defaultValue;
      }
      return key;
    },
  }),
}));
interface WrapperProps {
  children: ReactNode;
}
function createWrapper() {
  const queryClient = createTestQueryClient();
  const wrapper = ({ children }: WrapperProps) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  return { wrapper, queryClient };
}
describe("useProxyStatus", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_proxy_status") {
        return Promise.resolve({
          running: false,
          address: "127.0.0.1",
          port: 15721,
          active_connections: 0,
          total_requests: 0,
          success_requests: 0,
          failed_requests: 0,
          success_rate: 0,
          uptime_seconds: 0,
          current_provider: null,
          current_provider_id: null,
          last_request_at: null,
          last_error: null,
          failover_count: 0,
        });
      }
      if (command === "get_proxy_takeover_status") {
        return Promise.resolve({
          claude: false,
          codex: false,
          opencode: false,
        });
      }
      if (command === "start_proxy_server") {
        return Promise.resolve({
          address: "127.0.0.1",
          port: 15721,
          started_at: "2026-03-10T00:00:00Z",
        });
      }
      return Promise.resolve(null);
    });
  });
  it("keeps the established proxy query key shapes", () => {
    expect(proxyKeys.status).toEqual(["proxyStatus"]);
    expect(proxyKeys.takeoverStatus).toEqual(["proxyTakeoverStatus"]);
    expect(proxyKeys.globalConfig).toEqual(["globalProxyConfig"]);
    expect(proxyKeys.appConfig("claude")).toEqual(["appProxyConfig", "claude"]);
  });
  it("shows interpolated address and port after proxy server starts", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProxyStatus(), { wrapper });
    await waitFor(() => {
      expect(result.current.status).toBeDefined();
    });
    await act(async () => {
      await result.current.startProxyServer();
    });
    expect(toastSuccessMock).toHaveBeenCalledWith(
      "代理服务已启动 - 127.0.0.1:15721",
      { closeButton: true },
    );
    await act(async () => {
      await result.current.stopProxyServer();
    });
    expect(invokeMock).toHaveBeenCalledWith("start_proxy_server");
    expect(invokeMock).toHaveBeenCalledWith("stop_proxy_server");
  });

  it.each([false, true])(
    "refreshes mode and health observers after restore (failure=%s)",
    async (fails) => {
      const { wrapper, queryClient } = createWrapper();
      const failure = new Error("Codex config is locked");
      let restored = false;
      const defaultInvoke = invokeMock.getMockImplementation()!;
      invokeMock.mockImplementation((command: string) => {
        if (command === "stop_proxy_with_restore") {
          restored = true;
          return fails ? Promise.reject(failure) : Promise.resolve();
        }
        if (command === "get_proxy_takeover_status") {
          return Promise.resolve({
            claude: !restored,
            codex: fails || !restored,
          });
        }
        return defaultInvoke(command);
      });
      const health = renderHook(
        () =>
          useQuery({
            queryKey: ["providerHealth", "p1", "claude"],
            queryFn: async () => (restored && !fails ? null : "unhealthy"),
          }),
        { wrapper },
      );
      const { result } = renderHook(
        () => ({
          proxy: useProxyStatus(),
          mode: useQuery({
            queryKey: ["providers", "claude", "mode"],
            queryFn: async () => (restored ? "direct" : "route"),
          }),
          auth: useQuery({
            queryKey: ["managed-auth-status", "claude_official"],
            queryFn: async () =>
              restored ? "direct-account" : "route-account",
          }),
        }),
        { wrapper },
      );
      await waitFor(() => expect(result.current.mode.data).toBe("route"));
      await waitFor(() => expect(health.result.current.data).toBe("unhealthy"));
      await act(async () => {
        if (fails)
          await expect(result.current.proxy.stopWithRestore()).rejects.toBe(
            failure,
          );
        else await result.current.proxy.stopWithRestore();
      });
      await waitFor(() => {
        expect(result.current.proxy.takeoverStatus?.claude).toBe(false);
        expect(result.current.mode.data).toBe("direct");
        expect(health.result.current.data).toBe(fails ? "unhealthy" : null);
        expect(result.current.auth.data).toBe("direct-account");
      });
      expect(toastSuccessMock).toHaveBeenCalledTimes(fails ? 0 : 1);
      expect(toastErrorMock).toHaveBeenCalledTimes(fails ? 1 : 0);
      queryClient.clear();
    },
  );
});
