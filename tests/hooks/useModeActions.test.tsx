import type { ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { useModeActions } from "@/hooks/useModeActions";
import { createTestQueryClient } from "../utils/testQueryClient";
const mocks = vi.hoisted(() => ({
  takeover: vi.fn(),
  invalidate: vi.fn(),
  tray: vi.fn(),
}));
vi.mock("@/lib/api/proxy", () => ({
  proxyApi: { setProxyTakeoverForApp: mocks.takeover },
}));
vi.mock("@/lib/query/providerCache", () => ({
  invalidateProviderCaches: mocks.invalidate,
}));
vi.mock("@/lib/query/providerActions", () => ({
  refreshProviderTray: mocks.tray,
}));
vi.mock("@/lib/toast", () => ({
  toast: { success: vi.fn(), warning: vi.fn() },
}));
describe("useModeActions partial failure", () => {
  it("refreshes the originating app after a failed enter and preserves the error", async () => {
    const client = createTestQueryClient();
    const failure = new Error("client files partially written");
    mocks.takeover.mockRejectedValueOnce(failure);
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useModeActions("claude"), { wrapper });
    await act(async () => {
      await expect(result.current.enter("route", "p1")).rejects.toBe(failure);
    });
    expect(mocks.invalidate).toHaveBeenCalledWith(client, "claude", "mode");
    expect(mocks.tray).toHaveBeenCalled();
    expect(result.current.pending).toBe(false);
    client.clear();
  });
});
