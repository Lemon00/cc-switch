import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useStartupFailures } from "@/hooks/useStartupFailures";
import { useTrayNavigation } from "@/hooks/useTrayNavigation";
const mocks = vi.hoisted(() => ({
  listen: vi.fn(),
  invoke: vi.fn(),
  readFailures: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@/lib/api/proxy", () => ({
  proxyApi: { takeStartupAttachFailures: mocks.readFailures },
}));
beforeEach(() => {
  vi.resetAllMocks();
  mocks.invoke.mockResolvedValue(null);
  mocks.readFailures.mockResolvedValue([]);
});
afterEach(() => vi.restoreAllMocks());
describe.each([
  { event: "startup-routing-settled", useSubscription: useStartupFailures },
  {
    event: "tray-navigate",
    useSubscription: () => useTrayNavigation(() => {}),
  },
])("startup listener $event", ({ event, useSubscription }) => {
  it.each(["throw", "reject"])(
    "survives cleanup failure (%s) and ignores queued notifications after unmount",
    async (mode) => {
      const error = new Error("IPC cleanup unavailable");
      const report = vi.spyOn(console, "error").mockImplementation(() => {});
      const off = vi.fn(() => {
        if (mode === "throw") throw error;
        return Promise.reject(error);
      });
      mocks.listen.mockResolvedValue(off);
      const { unmount } = renderHook(() => useSubscription());
      await waitFor(() => {
        expect(
          mocks.invoke.mock.calls.length + mocks.readFailures.mock.calls.length,
        ).toBe(1);
      });
      expect(() => unmount()).not.toThrow();
      await act(async () => mocks.listen.mock.calls[0][1]());
      expect(off).toHaveBeenCalledOnce();
      expect(
        mocks.invoke.mock.calls.length + mocks.readFailures.mock.calls.length,
      ).toBe(1);
      expect(report).toHaveBeenCalledWith(
        `Failed to unsubscribe ${event} event`,
        error,
      );
    },
  );
  it("handles failure releasing a subscription that arrives after unmount", async () => {
    let subscribe!: (off: () => void) => void;
    mocks.listen.mockReturnValue(
      new Promise((resolve) => {
        subscribe = resolve;
      }),
    );
    const error = new Error("late cleanup unavailable");
    const report = vi.spyOn(console, "error").mockImplementation(() => {});
    const off = vi.fn().mockRejectedValue(error);
    const { unmount } = renderHook(() => useSubscription());
    unmount();
    await act(async () => subscribe(off));
    expect(off).toHaveBeenCalledOnce();
    expect(report).toHaveBeenCalledWith(
      `Failed to unsubscribe ${event} event`,
      error,
    );
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(mocks.readFailures).not.toHaveBeenCalled();
  });
});
