import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AutoFailoverConfigPanel } from "@/components/proxy/AutoFailoverConfigPanel";
import type { AppProxyConfig } from "@/types/proxy";
const mocks = vi.hoisted(() => ({
  configs: {} as Record<string, AppProxyConfig | undefined>,
  save: vi.fn(),
}));
vi.mock("@/lib/query/proxy", () => ({
  useAppProxyConfig: (app: string) => ({
    data: mocks.configs[app],
    isLoading: false,
    error: null,
  }),
  useUpdateAppProxyConfig: () => ({
    mutateAsync: mocks.save,
    isPending: false,
  }),
}));
vi.mock("@/lib/query/failover", () => ({
  useAutoFailoverEnabled: () => ({ data: false }),
}));
vi.mock("@/lib/toast", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
function config(appType: string, maxRetries = 3): AppProxyConfig {
  return {
    appType,
    enabled: true,
    autoFailoverEnabled: false,
    maxRetries,
    streamingFirstByteTimeout: 60,
    streamingIdleTimeout: 120,
    nonStreamingTimeout: 600,
    circuitFailureThreshold: 5,
    circuitSuccessThreshold: 2,
    circuitTimeoutSeconds: 60,
    circuitErrorRateThreshold: 0.5,
    circuitMinRequests: 10,
  };
}
describe("AutoFailoverConfigPanel drafts", () => {
  beforeEach(() => {
    mocks.configs = { claude: config("claude"), codex: config("codex", 1) };
    mocks.save.mockReset().mockResolvedValue(undefined);
  });
  it("preserves edited values when a background refetch updates the config", async () => {
    const { rerender } = render(<AutoFailoverConfigPanel appType="claude" />);
    const input = screen.getByLabelText("最大重试次数");
    fireEvent.change(input, { target: { value: "7" } });
    mocks.configs.claude = config("claude", 4);
    rerender(<AutoFailoverConfigPanel appType="claude" />);
    expect(input).toHaveValue(7);
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(mocks.save).toHaveBeenCalledWith(
        expect.objectContaining({ appType: "claude", maxRetries: 7 }),
      ),
    );
  });
  it("resets to the newest server values when explicitly requested", () => {
    const { rerender } = render(<AutoFailoverConfigPanel appType="claude" />);
    fireEvent.change(screen.getByLabelText("最大重试次数"), {
      target: { value: "7" },
    });
    mocks.configs.claude = config("claude", 4);
    rerender(<AutoFailoverConfigPanel appType="claude" />);
    fireEvent.click(screen.getByRole("button", { name: "重置" }));
    expect(screen.getByLabelText("最大重试次数")).toHaveValue(4);
  });
  it("does not carry a draft into another app or save before its config arrives", () => {
    const { rerender } = render(<AutoFailoverConfigPanel appType="claude" />);
    fireEvent.change(screen.getByLabelText("最大重试次数"), {
      target: { value: "7" },
    });
    mocks.configs.codex = undefined;
    rerender(<AutoFailoverConfigPanel appType="codex" />);
    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
    mocks.configs.codex = config("codex", 1);
    rerender(<AutoFailoverConfigPanel appType="codex" />);
    expect(screen.getByLabelText("最大重试次数")).toHaveValue(1);
  });
});
