import { beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
const m = vi.hoisted(() => ({
  render: vi.fn(),
  invoke: vi.fn(),
  message: vi.fn(),
  exit: vi.fn(),
  activity: vi.fn(),
  report: vi.fn(),
  listen: vi.fn(),
}));
vi.mock("react-dom/client", () => ({
  default: { createRoot: () => ({ render: m.render }) },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: m.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: m.listen,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ message: m.message }));
vi.mock("@tauri-apps/plugin-process", () => ({ exit: m.exit }));
vi.mock("@/App", () => ({ default: () => null }));
vi.mock("@/components/DatabaseUpgrade", () => ({
  DatabaseUpgrade: () => null,
}));
vi.mock("@/contexts/UpdateContext", () => ({ UpdateProvider: () => null }));
vi.mock("@/components/theme-provider", () => ({ ThemeProvider: () => null }));
vi.mock("@/components/ui/tooltip", () => ({ TooltipProvider: () => null }));
vi.mock("@/components/ui/sonner", () => ({ Toaster: () => null }));
vi.mock("@/lib/query", () => ({
  queryClient: { fetchQuery: vi.fn(async () => ({})) },
}));
vi.mock("@/lib/api", () => ({ settingsApi: { get: vi.fn() } }));
vi.mock("@/i18n", () => ({
  default: { t: (key: string) => key },
  syncLanguageFromSettings: vi.fn(),
}));
vi.mock("@/lib/frontendLogger", () => ({
  installGlobalErrorHandlers: vi.fn(),
  reportFrontendError: m.report,
}));
vi.mock("@/lib/modelsDevAutoSync", () => ({
  MODELS_DEV_SYNC_CONFIG_QUERY_KEY: [],
  syncModelsDevPricingOnStartup: vi.fn(async () => ({ skipped: true })),
}));
vi.mock("@/lib/windowActivity", () => ({
  initializeWindowActivity: m.activity,
}));
vi.mock("@/lib/inputModality", () => ({ initializeInputModality: vi.fn() }));
vi.mock("@/lib/disableContextMenu", () => ({ disableContextMenu: vi.fn() }));
beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  m.message.mockResolvedValue(undefined);
  m.exit.mockResolvedValue(undefined);
  m.listen.mockResolvedValue(() => {});
});
describe("startup boundary", () => {
  it("does not overwrite a fatal event with a later successful startup response", async () => {
    let resolve!: (value: null) => void;
    m.invoke.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    await import("@/main");
    const onError = m.listen.mock.calls.find(
      ([event]) => event === "configLoadError",
    )![1];
    await onError({ payload: { error: "late startup failure" } });
    resolve(null);
    await new Promise((done) => setTimeout(done, 0));
    expect(m.activity).not.toHaveBeenCalled();
    expect(
      m.render.mock.calls[m.render.mock.calls.length - 1][0].props.role,
    ).toBe("alert");
  });
  it("keeps fatal recovery visible and still requests exit when the native dialog fails", async () => {
    m.invoke.mockResolvedValue({
      kind: "db_version_check_failed",
      error: "fixture",
    });
    m.message.mockRejectedValue(new Error("dialog unavailable"));
    m.exit.mockRejectedValue(new Error("exit unavailable"));
    await import("@/main");
    await waitFor(() => expect(m.exit).toHaveBeenCalledWith(1));
    expect(m.activity).not.toHaveBeenCalled();
    expect(
      m.render.mock.calls[m.render.mock.calls.length - 1]?.[0].props.role,
    ).toBe("alert");
  });
  it("does not mount business UI when startup status cannot be read", async () => {
    m.invoke.mockRejectedValue(new Error("IPC unavailable"));
    await import("@/main");
    await waitFor(() => expect(m.exit).toHaveBeenCalledWith(1));
    expect(m.activity).not.toHaveBeenCalled();
  });
  it("waits for backend initialization before mounting business UI", async () => {
    let resolve!: (value: null) => void;
    m.invoke.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    await import("@/main");
    expect(m.activity).not.toHaveBeenCalled();
    expect(m.render.mock.calls[0][0].props.role).toBe("status");
    resolve(null);
    await waitFor(() => expect(m.activity).toHaveBeenCalledOnce());
    expect(m.exit).not.toHaveBeenCalled();
  });
});
