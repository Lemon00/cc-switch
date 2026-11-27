import type { ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useUpdateProviderMutation } from "@/lib/query/mutations";
import { usageKeys } from "@/lib/query/usage";
import type { Provider } from "@/types";
const apiMocks = vi.hoisted(() => ({
  update: vi.fn(),
  refresh: vi.fn(),
  updateTrayMenu: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  providersApi: {
    update: (...args: unknown[]) => apiMocks.update(...args),
    refresh: (...args: unknown[]) => apiMocks.refresh(...args),
    updateTrayMenu: (...args: unknown[]) => apiMocks.updateTrayMenu(...args),
  },
  sessionsApi: {},
  settingsApi: {},
}));
vi.mock("@/hooks/useHermes", () => ({
  invalidateHermesProviderCaches: vi.fn(),
}));
vi.mock("@/hooks/useOpenClaw", () => ({
  openclawKeys: {
    health: ["health"],
  },
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (
      _key: string,
      options?: {
        defaultValue?: string;
      },
    ) => options?.defaultValue ?? _key,
  }),
}));
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  return { wrapper, invalidateSpy };
}
function createProvider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: "provider-1",
    name: "Test Provider",
    settingsConfig: {},
    ...overrides,
  };
}
beforeEach(() => {
  apiMocks.update.mockReset().mockResolvedValue(true);
  apiMocks.refresh.mockReset().mockResolvedValue({});
  apiMocks.updateTrayMenu.mockReset().mockResolvedValue(true);
});
describe("useUpdateProviderMutation", () => {
  it("invalidates the updated provider usage query", async () => {
    const { wrapper, invalidateSpy } = createWrapper();
    const provider = createProvider({ id: "provider-b" });
    const { result } = renderHook(() => useUpdateProviderMutation("codex"), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync({ provider });
    });
    expect(apiMocks.update).toHaveBeenCalledWith(
      provider,
      "codex",
      undefined,
      undefined,
    );
    expect(apiMocks.updateTrayMenu).toHaveBeenCalledTimes(1);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["providers", "codex"],
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: usageKeys.script("provider-b", "codex"),
    });
    expect(invalidateSpy).not.toHaveBeenCalledWith({
      queryKey: usageKeys.all,
    });
  });
  it("refreshes DSH provider caches even when an update fails", async () => {
    apiMocks.update.mockRejectedValueOnce(new Error("conflict"));
    const { wrapper, invalidateSpy } = createWrapper();
    const provider = createProvider({ id: "pi-provider" });
    const { result } = renderHook(() => useUpdateProviderMutation("dsh"), {
      wrapper,
    });
    await act(async () => {
      await expect(result.current.mutateAsync({ provider })).rejects.toThrow(
        "conflict",
      );
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ["providers", "dsh"],
    });
  });
});
