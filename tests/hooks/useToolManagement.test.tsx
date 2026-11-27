import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ToolInstallationReport } from "@/lib/api/settings";
const mocks = vi.hoisted(() => ({
  getToolVersions: vi.fn(),
  listToolInstallations: vi.fn(),
  probeToolInstallations: vi.fn(),
  runToolLifecycleAction: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/api", () => ({ settingsApi: mocks }));
vi.mock("@/lib/toast", () => ({ toast: mocks }));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function report(tool: string, path = "new-install"): ToolInstallationReport {
  return {
    tool,
    installs: [
      {
        path,
        version: "2.0.0",
        runnable: true,
        error: null,
        source: "system",
        is_path_default: true,
      },
    ],
    is_conflict: false,
    needs_confirmation: false,
    command: "",
    anchored: true,
    unmanaged: false,
  };
}
const versions = (tools: string[]) =>
  tools.map((name) => ({
    name,
    version: "2.0.0",
    latest_version: "2.0.0",
    error: null,
    installed_but_broken: false,
    env_type: "windows",
    wsl_distro: null,
    update_blocked_by_node: null,
  }));
describe("tool management request ownership", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mocks.getToolVersions
      .mockReset()
      .mockImplementation(async (tools: string[]) => versions(tools));
    mocks.listToolInstallations
      .mockReset()
      .mockImplementation(async (tools: string[]) =>
        tools.map((tool) => report(tool)),
      );
    mocks.probeToolInstallations
      .mockReset()
      .mockImplementation(async (tools: string[]) =>
        tools.map((tool) => report(tool)),
      );
    mocks.runToolLifecycleAction.mockReset().mockResolvedValue(undefined);
  });
  it("refreshes install locations after installation and discards an older full scan", async () => {
    const pending = deferred<ToolInstallationReport[]>();
    mocks.listToolInstallations.mockReturnValueOnce(pending.promise);
    const { useToolManagement } = await import(
      "@/components/apps/useToolManagement"
    );
    const { result } = renderHook(useToolManagement);
    await waitFor(() => expect(result.current.isLoadingTools).toBe(false));
    await act(async () => {
      await result.current.handleRunToolAction(["codex"], "install");
    });
    expect(result.current.installReports?.codex?.installs[0].path).toBe(
      "new-install",
    );
    await act(async () => {
      pending.resolve([report("codex", "old-install"), report("claude")]);
    });
    expect(result.current.installReports?.codex?.installs[0].path).toBe(
      "new-install",
    );
    expect(result.current.installReports?.claude).toBeDefined();
  });
  it("does not mark a failed partial version scan as fresh for ten minutes", async () => {
    const defaultProbe = mocks.getToolVersions.getMockImplementation()!;
    let failed = false;
    mocks.getToolVersions.mockImplementation(async (tools: string[]) => {
      if (tools[0] === "codex" && !failed) {
        failed = true;
        throw new Error("probe unavailable");
      }
      return defaultProbe(tools);
    });
    const { useToolManagement } = await import(
      "@/components/apps/useToolManagement"
    );
    const first = renderHook(useToolManagement);
    await waitFor(() =>
      expect(first.result.current.isLoadingTools).toBe(false),
    );
    expect(first.result.current.lastCheckedAt).toBeNull();
    first.unmount();
    const second = renderHook(useToolManagement);
    await waitFor(() =>
      expect(
        second.result.current.toolVersionByName.get("codex")?.version,
      ).toBe("2.0.0"),
    );
    expect(second.result.current.lastCheckedAt).not.toBeNull();
  });
  it("retains WSL preferences when the page remounts", async () => {
    const { useToolManagement } = await import(
      "@/components/apps/useToolManagement"
    );
    const first = renderHook(useToolManagement);
    await waitFor(() =>
      expect(first.result.current.isLoadingTools).toBe(false),
    );
    await act(async () => {
      await first.result.current.handleToolShellChange("codex", "bash");
    });
    first.unmount();
    const second = renderHook(useToolManagement);
    expect(second.result.current.wslShellByTool.codex.wslShell).toBe("bash");
    await act(async () => {
      await second.result.current.handleRunToolAction(
        ["codex", "codex"],
        "install",
      );
    });
    expect(mocks.runToolLifecycleAction).toHaveBeenCalledTimes(1);
    expect(mocks.runToolLifecycleAction).toHaveBeenCalledWith(
      ["codex"],
      "install",
      { codex: { wslShell: "bash" } },
    );
  });
  it("clears loading when a background request supersedes a page request", async () => {
    const pending = deferred<ReturnType<typeof versions>>();
    mocks.getToolVersions.mockImplementation(async (tools: string[]) =>
      tools.length === 1 && tools[0] === "codex"
        ? pending.promise
        : versions(tools),
    );
    const { useToolManagement, checkToolUpdatesInBackground } = await import(
      "@/components/apps/useToolManagement"
    );
    const { result } = renderHook(useToolManagement);
    await waitFor(() => expect(result.current.loadingTools.codex).toBe(true));
    await act(async () => {
      await checkToolUpdatesInBackground();
    });
    expect(result.current.loadingTools.codex).toBe(false);
    await act(async () => {
      pending.resolve(versions(["codex"]));
    });
    expect(result.current.loadingTools.codex).toBe(false);
  });
  it("keeps background scans from superseding post-install verification", async () => {
    mocks.getToolVersions.mockImplementation(async (tools: string[]) =>
      tools[0] === "codex" ? [] : versions(tools),
    );
    const { useToolManagement, checkToolUpdatesInBackground } = await import(
      "@/components/apps/useToolManagement"
    );
    const { result } = renderHook(useToolManagement);
    await waitFor(() => expect(result.current.isLoadingTools).toBe(false));
    const pending = deferred<ReturnType<typeof versions>>();
    mocks.getToolVersions.mockImplementation(async (tools: string[]) =>
      tools.length === 1 && tools[0] === "codex"
        ? pending.promise
        : versions(tools),
    );
    let operation!: Promise<void>;
    act(() => {
      operation = result.current.handleRunToolAction(["codex"], "install");
    });
    await waitFor(() => expect(result.current.loadingTools.codex).toBe(true));
    await act(async () => {
      await checkToolUpdatesInBackground();
    });
    expect(mocks.getToolVersions.mock.lastCall?.[0]).not.toContain("codex");
    expect(result.current.loadingTools.codex).toBe(true);
    await act(async () => {
      pending.resolve(versions(["codex"]));
      await operation;
    });
    expect(result.current.toolVersionByName.get("codex")?.version).toBe(
      "2.0.0",
    );
    expect(mocks.success).toHaveBeenCalledTimes(1);
    expect(mocks.warning).not.toHaveBeenCalled();
  });
  it("fills missing installation reports after a partial scan on remount", async () => {
    mocks.listToolInstallations.mockResolvedValueOnce([report("claude")]);
    const { useToolManagement } = await import(
      "@/components/apps/useToolManagement"
    );
    const first = renderHook(useToolManagement);
    await waitFor(() =>
      expect(first.result.current.installReports?.claude).toBeDefined(),
    );
    first.unmount();
    const second = renderHook(useToolManagement);
    await waitFor(() =>
      expect(second.result.current.installReports?.codex).toBeDefined(),
    );
    expect(mocks.listToolInstallations.mock.lastCall?.[0]).not.toContain(
      "claude",
    );
    expect(mocks.listToolInstallations.mock.lastCall?.[0]).toContain("codex");
  });
});
