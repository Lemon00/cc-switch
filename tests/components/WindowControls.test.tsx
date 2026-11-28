import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, act, cleanup, screen } from "@testing-library/react";
type ResizeCb = () => void;
const mockState = {
  isMaximizedDelayMs: 0,
  onResizedDelayMs: 0,
  shouldFailIsMaximized: false,
  maximized: false,
  reads: 0,
  cleanupFailure: "" as "" | "throw" | "reject",
  activeListeners: new Set<ResizeCb>(),
  reset() {
    this.isMaximizedDelayMs = 0;
    this.onResizedDelayMs = 0;
    this.shouldFailIsMaximized = false;
    this.maximized = false;
    this.reads = 0;
    this.cleanupFailure = "";
    this.activeListeners.clear();
  },
  triggerResize() {
    for (const cb of [...this.activeListeners]) {
      cb();
    }
  },
};
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => {
      mockState.reads++;
      const maximized = mockState.maximized;
      if (mockState.isMaximizedDelayMs > 0) {
        await new Promise((r) => setTimeout(r, mockState.isMaximizedDelayMs));
      }
      if (mockState.shouldFailIsMaximized) {
        throw new Error("simulated isMaximized error");
      }
      return maximized;
    },
    onResized: async (cb: ResizeCb) => {
      if (mockState.onResizedDelayMs > 0) {
        await new Promise((r) => setTimeout(r, mockState.onResizedDelayMs));
      }
      mockState.activeListeners.add(cb);
      return () => {
        mockState.activeListeners.delete(cb);
        if (mockState.cleanupFailure === "throw")
          throw new Error("cleanup failed");
        if (mockState.cleanupFailure === "reject")
          return Promise.reject(new Error("cleanup failed"));
      };
    },
    minimize: async () => {},
    toggleMaximize: async () => {},
    close: async () => {},
  }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (k: string) => k }),
}));
import { WindowControls } from "@/components/shell/WindowControls";
describe("WindowControls", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockState.reset();
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });
  it("reads changes made while the resize subscription is being registered", async () => {
    mockState.onResizedDelayMs = 10;
    render(<WindowControls />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5);
      mockState.maximized = true;
      mockState.triggerResize();
      await vi.advanceTimersByTimeAsync(5);
    });
    expect(
      screen.getByRole("button", { name: "header.windowRestore" }),
    ).toBeInTheDocument();
  });
  it("coalesces resize bursts and does not apply an obsolete in-flight snapshot", async () => {
    render(<WindowControls />);
    await act(async () => vi.advanceTimersByTimeAsync(0));
    expect(mockState.reads).toBe(1);
    mockState.isMaximizedDelayMs = 20;
    act(() => mockState.triggerResize());
    mockState.maximized = true;
    act(() => {
      for (let i = 0; i < 30; i++) mockState.triggerResize();
    });
    expect(mockState.reads).toBe(2);
    await act(async () => vi.advanceTimersByTimeAsync(40));
    expect(mockState.reads).toBe(3);
    expect(
      screen.getByRole("button", { name: "header.windowRestore" }),
    ).toBeInTheDocument();
  });
  it.each(["throw", "reject"] as const)(
    "handles listener cleanup failure (%s)",
    async (mode) => {
      const report = vi.spyOn(console, "error").mockImplementation(() => {});
      const { unmount } = render(<WindowControls />);
      await act(async () => vi.advanceTimersByTimeAsync(0));
      mockState.cleanupFailure = mode;
      expect(() => unmount()).not.toThrow();
      await act(async () => vi.advanceTimersByTimeAsync(0));
      expect(report).toHaveBeenCalledWith(
        "Failed to unsubscribe tauri://resize event",
        expect.any(Error),
      );
    },
  );
  it("cleans up onResized listener when unmounted before registration resolves (StrictMode)", async () => {
    mockState.isMaximizedDelayMs = 2;
    mockState.onResizedDelayMs = 4;
    const { unmount } = render(
      <React.StrictMode>
        <WindowControls />
      </React.StrictMode>,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2);
    });
    expect(mockState.activeListeners.size).toBe(0);
    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4);
    });
    expect(mockState.activeListeners.size).toBe(0);
  });
  it("keeps one active listener in StrictMode and removes it on unmount", async () => {
    mockState.isMaximizedDelayMs = 2;
    mockState.onResizedDelayMs = 4;
    const { unmount } = render(
      <React.StrictMode>
        <WindowControls />
      </React.StrictMode>,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6);
    });
    expect(mockState.activeListeners.size).toBe(1);
    unmount();
    expect(mockState.activeListeners.size).toBe(0);
  });
  it("handles isMaximized errors during resize without unhandled rejections", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { unmount } = render(<WindowControls />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(mockState.activeListeners.size).toBe(1);
    mockState.shouldFailIsMaximized = true;
    mockState.triggerResize();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(errSpy).toHaveBeenCalled();
    unmount();
    errSpy.mockRestore();
  });
});
