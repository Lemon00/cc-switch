import { Suspense, type ComponentType } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  render,
  screen,
  waitFor,
  fireEvent,
  within,
} from "@testing-library/react";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { resetProviderState } from "../msw/state";
import { emitTauriEvent } from "../msw/tauriMocks";
import { server } from "../msw/server";
import { createTestQueryClient } from "../utils/testQueryClient";
import { hasUnsavedChanges } from "@/lib/unsavedChanges";
import type { Settings } from "@/types";
const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();
const skillsPanelMocks = vi.hoisted(() => ({
  initialViews: [] as string[],
}));
const workspaceMocks = vi.hoisted(() => ({ render: vi.fn() }));
vi.mock("@/components/workspace/WorkspaceFilesPanel", () => ({
  default: () => {
    workspaceMocks.render(localStorage.getItem("cc-switch-last-app"));
    return <div data-testid="workspace-panel" />;
  },
}));
const windowMocks = vi.hoisted(() => ({
  isLinux: vi.fn(() => false),
  isMaximized: vi.fn(),
  isFullscreen: vi.fn(),
  isResizable: vi.fn(),
  scaleFactor: vi.fn(),
  setDecorations: vi.fn(async () => {}),
  resizeListeners: new Set<() => void>(),
  onResized: vi.fn(),
  onScaleChanged: vi.fn(),
}));
vi.mock("@/lib/platform", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/platform")>()),
  isLinux: windowMocks.isLinux,
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => windowMocks,
}));
vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));
vi.mock("@/components/providers/ProviderList", () => ({
  ProviderList: ({
    providers,
    currentProviderId,
    onSwitch,
    onEdit,
    onDuplicate,
    onConfigureUsage,
    onOpenWebsite,
    onCreate,
    onDelete,
    onRemoveFromConfig,
  }: any) => (
    <div>
      <div data-testid="provider-list">{JSON.stringify(providers)}</div>
      <div data-testid="current-provider">{currentProviderId}</div>
      <button onClick={() => onSwitch(providers[currentProviderId])}>
        switch
      </button>
      <button onClick={() => onEdit(providers[currentProviderId])}>edit</button>
      <button onClick={() => onDuplicate(providers[currentProviderId])}>
        duplicate
      </button>
      <button onClick={() => onConfigureUsage(providers[currentProviderId])}>
        usage
      </button>
      <button onClick={() => onOpenWebsite("https://example.com")}>
        open-website
      </button>
      <button onClick={() => onDelete(Object.values(providers)[0])}>
        delete
      </button>
      <button onClick={() => onRemoveFromConfig?.(Object.values(providers)[0])}>
        remove
      </button>
      <button onClick={() => onCreate?.()}>create</button>
    </div>
  ),
}));
vi.mock("@/components/providers/AddProviderDialog", () => ({
  AddProviderDialog: ({ open, onOpenChange, onSubmit, appId }: any) =>
    open ? (
      <div data-testid="add-provider-dialog" data-app={appId}>
        <button
          onClick={() =>
            onSubmit({
              name: `New ${appId} Provider`,
              settingsConfig: {},
              category: "custom",
              sortIndex: 99,
            })
          }
        >
          confirm-add
        </button>
        <button onClick={() => onOpenChange(false)}>close-add</button>
      </div>
    ) : null,
}));
vi.mock("@/components/providers/EditProviderDialog", async () => {
  const { useUnsavedChangesTracker } = await vi.importActual<
    typeof import("@/lib/unsavedChanges")
  >("@/lib/unsavedChanges");
  // 和真的编辑页一样登记改动（真的由 FullScreenPanel 的 trackUnsavedChanges 负责）
  const Body = ({ provider, onSubmit, onOpenChange }: any) => (
    <div data-testid="edit-provider-dialog" {...useUnsavedChangesTracker()}>
      <input aria-label="edit-field" />
      <button
        onClick={() =>
          onSubmit({
            provider: {
              ...provider,
              name: `${provider.name}-edited`,
            },
            originalId: provider.id,
          })
        }
      >
        confirm-edit
      </button>
      <button onClick={() => onOpenChange(false)}>close-edit</button>
    </div>
  );
  return {
    EditProviderDialog: (props: any) =>
      props.open ? <Body {...props} /> : null,
  };
});
vi.mock("@/components/UsageScriptModal", () => ({
  default: ({ isOpen, provider, onSave, onClose }: any) =>
    isOpen ? (
      <div data-testid="usage-modal">
        <span data-testid="usage-provider">{provider?.id}</span>
        <button onClick={() => onSave("script-code")}>save-script</button>
        <button onClick={() => onClose()}>close-usage</button>
      </div>
    ) : null,
}));
vi.mock("@/components/ConfirmDialog", () => ({
  ConfirmDialog: ({ isOpen, message, onConfirm, onCancel }: any) =>
    isOpen ? (
      <div data-testid="confirm-dialog">
        <div data-testid="confirm-message">{message}</div>
        <button onClick={() => onConfirm()}>confirm-delete</button>
        <button onClick={() => onCancel()}>cancel-delete</button>
      </div>
    ) : null,
}));
vi.mock("@/contexts/UpdateContext", () => ({
  useUpdate: () => ({ hasUpdate: false, updateInfo: null }),
}));
// 设置页要 ThemeProvider，这里只看导航进去时发生了什么
vi.mock("@/components/settings/SettingsPage", () => ({
  SettingsPage: () => <div data-testid="settings-page" />,
}));
vi.mock("@/components/skills/UnifiedSkillsPanel", () => ({
  // v7：Skills 的页头（添加 / 检查更新 / 存储与同步）在面板自己里面
  default: ({ initialView }: { initialView?: string }) => {
    skillsPanelMocks.initialViews.push(initialView ?? "installed");
    return <div data-testid="unified-skills-panel">{initialView}</div>;
  },
}));
vi.mock("@/components/mcp/McpPanel", () => ({
  default: ({ open, onOpenChange }: any) =>
    open ? (
      <div data-testid="mcp-panel">
        <button onClick={() => onOpenChange(false)}>close-mcp</button>
      </div>
    ) : (
      <button onClick={() => onOpenChange(true)}>open-mcp</button>
    ),
}));
vi.mock("@/components/mcp/UnifiedMcpPanel", async () => {
  const { useUnsavedChangesTracker } = await vi.importActual<
    typeof import("@/lib/unsavedChanges")
  >("@/lib/unsavedChanges");
  // 当作一直开着的 MCP 编辑页：输入即登记未保存的修改
  const Body = ({ onInteractionBlockedChange }: any) => (
    <div data-testid="mcp-editor" {...useUnsavedChangesTracker()}>
      <input aria-label="mcp-field" />
      <button onClick={() => onInteractionBlockedChange(true)}>
        mark-mcp-busy
      </button>
      <button onClick={() => onInteractionBlockedChange(false)}>
        clear-mcp-busy
      </button>
    </div>
  );
  return { default: Body };
});
/** 侧栏里的应用行（v7 侧栏取代了原来页头的应用切换器） */
const sidebarApp = (name: string) =>
  // 首次启动提示是模态对话框，会把侧栏标成 aria-hidden，所以带上 hidden
  within(document.querySelector("nav") as HTMLElement).getByRole("button", {
    name,
    hidden: true,
  });
const renderApp = (AppComponent: ComponentType) => {
  const client = createTestQueryClient();
  const result = render(
    <QueryClientProvider client={client}>
      <Suspense fallback={<div data-testid="loading">loading</div>}>
        <AppComponent />
      </Suspense>
    </QueryClientProvider>,
  );
  return { ...result, client };
};
vi.mock("@/components/providers/mode/SwitchModePanel", () => ({
  SwitchModePanel: ({ providers, startupFailure }: any) => (
    <div>
      <div data-testid="provider-list">{JSON.stringify(providers)}</div>
      <div data-testid="startup-failure">
        {JSON.stringify(startupFailure ?? null)}
      </div>
    </div>
  ),
}));
describe("Startup navigation regressions", () => {
  beforeEach(() => {
    resetProviderState();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
    skillsPanelMocks.initialViews = [];
    workspaceMocks.render.mockClear();
    windowMocks.isLinux.mockReturnValue(false);
    windowMocks.isMaximized.mockResolvedValue(false);
    windowMocks.isFullscreen.mockResolvedValue(false);
    windowMocks.isResizable.mockResolvedValue(true);
    windowMocks.scaleFactor.mockResolvedValue(1);
    windowMocks.resizeListeners.clear();
    const listen = async (handler: () => void) => {
      windowMocks.resizeListeners.add(handler);
      return () => windowMocks.resizeListeners.delete(handler);
    };
    windowMocks.onResized.mockImplementation(listen);
    windowMocks.onScaleChanged.mockImplementation(listen);
    localStorage.removeItem("cc-switch-last-view");
    localStorage.removeItem("cc-switch-last-app");
  });
  it("does not mount a restored app-specific page for a different app", async () => {
    const { default: App } = await import("@/App");
    localStorage.setItem("cc-switch-last-app", "codex");
    localStorage.setItem("cc-switch-last-view", "workspace");
    renderApp(App);
    await waitFor(() =>
      expect(screen.getByTestId("provider-list")).toHaveTextContent("codex-1"),
    );
    expect(workspaceMocks.render).not.toHaveBeenCalled();
    expect(localStorage.getItem("cc-switch-last-view")).toBe("providers");
  }, 15000);
  it("falls back from retired saved apps and their pages even when legacy visibility is enabled", async () => {
    localStorage.setItem("cc-switch-last-app", "openclaw");
    localStorage.setItem("cc-switch-last-view", "workspace");
    const { default: App } = await import("@/App");
    const { client } = renderApp(App);
    await screen.findByTestId("provider-list");
    await waitFor(() =>
      expect(client.getQueryData(["settings"])).toBeDefined(),
    );
    workspaceMocks.render.mockClear();
    act(() => {
      const previous = client.getQueryData<Settings>(["settings"]);
      client.setQueryData(["settings"], {
        ...previous,
        visibleApps: { ...previous?.visibleApps },
      });
    });
    await waitFor(() =>
      expect(screen.getByTestId("provider-list")).toHaveTextContent("claude-1"),
    );
    expect(workspaceMocks.render).not.toHaveBeenCalledWith("claude");
    expect(localStorage.getItem("cc-switch-last-view")).toBe("providers");
  }, 15000);
  it("retains dirty state when a pending navigation becomes blocked", async () => {
    const { default: App } = await import("@/App");
    renderApp(App);
    await screen.findByTestId("provider-list");
    fireEvent.click(sidebarApp("MCP"));
    fireEvent.input(await screen.findByLabelText("mcp-field"), {
      target: { value: "draft" },
    });
    fireEvent.click(sidebarApp("Codex"));
    await screen.findByTestId("confirm-dialog");
    fireEvent.click(screen.getByText("mark-mcp-busy"));
    fireEvent.click(screen.getByText("confirm-delete"));
    expect(hasUnsavedChanges()).toBe(true);
    expect(screen.getByTestId("mcp-editor")).toBeInTheDocument();
    expect(screen.getByTestId("confirm-dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByText("clear-mcp-busy"));
    fireEvent.click(screen.getByText("confirm-delete"));
    await waitFor(() =>
      expect(screen.getByTestId("provider-list")).toHaveTextContent("codex-1"),
    );
    expect(hasUnsavedChanges()).toBe(false);
  }, 15000);
  it("keeps an open Add form bound to its app when visibility settings change", async () => {
    let pending: unknown = null;
    let takes = 0;
    server.use(
      http.post("http://tauri.local/take_tray_navigation", () => {
        takes++;
        const value = pending;
        pending = null;
        return HttpResponse.json(value as any);
      }),
    );
    const { default: App } = await import("@/App");
    const { client } = renderApp(App);
    await waitFor(() => expect(takes).toBe(1));
    pending = { app: "codex", intent: "add" };
    act(() => emitTauriEvent("tray-navigate", null));
    expect(await screen.findByTestId("add-provider-dialog")).toHaveAttribute(
      "data-app",
      "codex",
    );
    act(() => {
      const previous = client.getQueryData<Settings>(["settings"]);
      client.setQueryData(["settings"], {
        ...previous,
        visibleApps: { ...previous?.visibleApps, codex: false },
      });
    });
    expect(screen.getByTestId("add-provider-dialog")).toHaveAttribute(
      "data-app",
      "codex",
    );
    fireEvent.click(screen.getByText("close-add"));
    await waitFor(() =>
      expect(localStorage.getItem("cc-switch-last-app")).toBe("claude"),
    );
  }, 15000);
  it("ignores the whole tray Add intent while management is busy", async () => {
    let pending: unknown = null;
    let takes = 0;
    server.use(
      http.post("http://tauri.local/take_tray_navigation", () => {
        takes++;
        const value = pending;
        pending = null;
        return HttpResponse.json(value as any);
      }),
    );
    const { default: App } = await import("@/App");
    renderApp(App);
    await waitFor(() => expect(takes).toBe(1));
    await waitFor(() =>
      expect(screen.getByTestId("provider-list").textContent).toContain(
        "claude-1",
      ),
    );
    fireEvent.click(sidebarApp("MCP"));
    fireEvent.click(await screen.findByText("mark-mcp-busy"));
    pending = { app: "codex", intent: "add" };
    act(() => emitTauriEvent("tray-navigate", null));
    await waitFor(() => expect(takes).toBe(2));
    expect(screen.queryByTestId("add-provider-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("mcp-editor")).toBeInTheDocument();
  }, 15000);
  it("shows failures delivered after the initial startup snapshot", async () => {
    let failures: unknown[] = [];
    let takes = 0;
    server.use(
      http.post("http://tauri.local/take_startup_attach_failures", () => {
        takes++;
        const result = failures;
        return HttpResponse.json(result);
      }),
    );
    const { default: App } = await import("@/App");
    renderApp(App);
    await waitFor(() => expect(takes).toBe(1));
    await waitFor(() =>
      expect(screen.getByTestId("provider-list").textContent).toContain(
        "claude-1",
      ),
    );
    failures = [
      { appType: "codex", stack: false, error: "fixture: port busy" },
    ];
    act(() => emitTauriEvent("startup-routing-settled", null));
    await waitFor(() => expect(takes).toBe(2));
    fireEvent.click(sidebarApp("Codex"));
    await waitFor(() =>
      expect(screen.getByTestId("provider-list").textContent).toContain(
        "codex-1",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("startup-failure")).toHaveTextContent(
        "fixture: port busy",
      ),
    );
    expect(takes).toBe(2);
    expect(failures).toHaveLength(1);
  }, 15000);
});
