import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DatabaseUpgrade } from "@/components/DatabaseUpgrade";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  exit: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-process", () => ({ exit: mocks.exit }));
vi.mock("react-i18next", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-i18next")>()),
  useTranslation: () => ({ t: (key: string) => key }),
}));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "check_app_update_available") return "9.0.0";
    if (command === "install_update_and_restart") return false;
    return undefined;
  });
  mocks.listen.mockReset().mockResolvedValue(vi.fn());
  mocks.exit.mockReset().mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());
describe("database upgrade recovery", () => {
  it.each(["throw", "reject"])(
    "keeps the installation error and permits retry when listener cleanup fails (%s)",
    async (mode) => {
      const cleanupError = new Error("listener cleanup failed");
      const report = vi.spyOn(console, "error").mockImplementation(() => {});
      const off = vi.fn(() => {
        if (mode === "throw") throw cleanupError;
        return Promise.reject(cleanupError);
      });
      mocks.listen.mockResolvedValueOnce(off);
      let installs = 0;
      mocks.invoke.mockImplementation(async (command: string) => {
        if (command === "check_app_update_available") return "9.0.0";
        if (command === "install_update_and_restart") {
          installs++;
          if (installs === 1) throw new Error("download failed");
          return false;
        }
      });
      render(<DatabaseUpgrade payload={{}} />);
      fireEvent.click(await screen.findByText("dbUpgrade.upgradeNow"));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "download failed",
      );
      await waitFor(() =>
        expect(report).toHaveBeenCalledWith(
          "Failed to unsubscribe update-download-progress event",
          cleanupError,
        ),
      );
      fireEvent.click(screen.getByText("dbUpgrade.retry"));
      await screen.findByText("dbUpgrade.incompatibleTitle");
      expect(installs).toBe(2);
      expect(off).toHaveBeenCalledOnce();
    },
  );
  it("ignores progress from a previous attempt after retry", async () => {
    const callbacks: ((event: {
      payload: {
        downloaded: number;
        total: number;
      };
    }) => void)[] = [];
    mocks.listen.mockImplementation(async (_event, callback) => {
      callbacks.push(callback);
      return vi.fn();
    });
    let finish!: (value: boolean) => void;
    let installs = 0;
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "check_app_update_available") return "9.0.0";
      if (command === "install_update_and_restart") {
        if (++installs === 1) throw new Error("download failed");
        return new Promise<boolean>((resolve) => {
          finish = resolve;
        });
      }
    });
    render(<DatabaseUpgrade payload={{}} />);
    fireEvent.click(await screen.findByText("dbUpgrade.upgradeNow"));
    fireEvent.click(await screen.findByText("dbUpgrade.retry"));
    await waitFor(() => expect(installs).toBe(2));
    act(() => callbacks[1]({ payload: { downloaded: 10, total: 100 } }));
    expect(screen.getByText("10%")).toBeInTheDocument();
    act(() => callbacks[0]({ payload: { downloaded: 90, total: 100 } }));
    expect(screen.getByText("10%")).toBeInTheDocument();
    await act(async () => finish(false));
  });
  it("releases a late listener and never starts installation after unmount", async () => {
    let resolve!: (off: () => void) => void;
    mocks.listen.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const view = render(
      <DatabaseUpgrade payload={{ kind: "db_version_too_new" }} />,
    );
    fireEvent.click(await screen.findByText("dbUpgrade.upgradeNow"));
    view.unmount();
    const off = vi.fn();
    await act(async () => {
      resolve(off);
    });
    expect(off).toHaveBeenCalledOnce();
    expect(mocks.invoke).not.toHaveBeenCalledWith("install_update_and_restart");
  });
  it("releases the listener when installation fails and permits retry", async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "check_app_update_available") return "9.0.0";
      throw new Error("download failed");
    });
    const off = vi.fn();
    mocks.listen.mockResolvedValue(off);
    render(<DatabaseUpgrade payload={{}} />);
    fireEvent.click(await screen.findByText("dbUpgrade.upgradeNow"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "download failed",
    );
    expect(off).toHaveBeenCalledOnce();
    mocks.invoke.mockResolvedValue(false);
    fireEvent.click(screen.getByText("dbUpgrade.retry"));
    await screen.findByText("dbUpgrade.incompatibleTitle");
    expect(off).toHaveBeenCalledTimes(2);
  });
  it("shows folder and exit failures without replacing the recovery phase", async () => {
    render(<DatabaseUpgrade payload={{}} />);
    await screen.findByText("dbUpgrade.upgradeNow");
    mocks.invoke.mockRejectedValue(new Error("folder denied"));
    fireEvent.click(screen.getByText("dbUpgrade.openConfigDir"));
    expect(await screen.findByRole("alert")).toHaveTextContent("folder denied");
    expect(screen.getByText("dbUpgrade.upgradeNow")).toBeInTheDocument();
    mocks.exit.mockRejectedValue(new Error("exit denied"));
    fireEvent.click(screen.getByText("dbUpgrade.quit"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("exit denied"),
    );
  });
});
