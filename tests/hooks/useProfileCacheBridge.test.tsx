import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useProfileCacheBridge } from "@/hooks/useProfileCacheBridge";
import { useApplyProfileMutation } from "@/lib/query/profiles";
import { proxyKeys } from "@/lib/query/proxy";
import { profilesApi } from "@/lib/api";
import { emitTauriEvent } from "../msw/tauriMocks";
vi.mock("@/lib/api", () => ({
  profilesApi: { apply: vi.fn().mockResolvedValue([]) },
  providersApi: { updateTrayMenu: vi.fn() },
}));
vi.mock("@/lib/toast", () => ({
  toast: { success: vi.fn(), warning: vi.fn(), error: vi.fn() },
}));
afterEach(cleanup);
describe("profile cache coordination", () => {
  it("refreshes partial changes when applying a profile fails", async () => {
    vi.mocked(profilesApi.apply).mockRejectedValueOnce(
      new Error(
        "Failed to persist the current profile after switching providers",
      ),
    );
    const client = new QueryClient();
    const keys = [
      ["profiles"],
      ["mcp", "all"],
      ["skills", "installed"],
      ["prompts", "list", "codex"],
      ["providers", "codex"],
      proxyKeys.status,
      proxyKeys.takeoverStatus,
    ];
    keys.forEach((key) => client.setQueryData(key, {}));
    client.setQueryData(["providers", "claude"], {});
    const wrapper = ({ children }: PropsWithChildren) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result, unmount } = renderHook(() => useApplyProfileMutation(), {
      wrapper,
    });
    await act(async () => {
      await expect(
        result.current.mutateAsync({ id: "profile", scope: "codex" }),
      ).rejects.toThrow("Failed to persist");
    });
    keys.forEach((key) =>
      expect(
        client.getQueryState(key)?.isInvalidated,
        JSON.stringify(key),
      ).toBe(true),
    );
    expect(client.getQueryState(["providers", "claude"])?.isInvalidated).toBe(
      false,
    );
    unmount();
    client.clear();
  });
  it.each(["event", "mutation"])(
    "refreshes the same state through the %s entry",
    async (entry) => {
      const client = new QueryClient();
      const keys = [
        ["profiles"],
        ["mcp", "all"],
        ["skills", "installed"],
        ["prompts", "list", "codex"],
        ["providers", "codex"],
        ["providers", "codex", "mode"],
        proxyKeys.status,
        proxyKeys.takeoverStatus,
      ];
      keys.forEach((key) => client.setQueryData(key, {}));
      client.setQueryData(["providers", "claude"], {});
      const wrapper = ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      );
      const { result, unmount } = renderHook(
        () => {
          useProfileCacheBridge();
          return useApplyProfileMutation();
        },
        { wrapper },
      );
      await act(async () => {});
      await act(async () => {
        if (entry === "event")
          emitTauriEvent("profile-applied", { scope: "codex" });
        else
          await result.current.mutateAsync({ id: "profile", scope: "codex" });
      });
      keys.forEach((key) =>
        expect(
          client.getQueryState(key)?.isInvalidated,
          JSON.stringify(key),
        ).toBe(true),
      );
      expect(client.getQueryState(["providers", "claude"])?.isInvalidated).toBe(
        false,
      );
      unmount();
      client.clear();
    },
  );
});
