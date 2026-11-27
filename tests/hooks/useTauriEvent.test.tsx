import { act, renderHook } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTauriEvent } from "@/hooks/useTauriEvent";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const unlisten = vi.fn();
const emit = (payload: string, subscription = 0) =>
  vi.mocked(listen).mock.calls[subscription][1]({
    event: "changed",
    id: subscription,
    payload,
  });
describe("useTauriEvent", () => {
  beforeEach(() => {
    vi.mocked(listen).mockReset().mockResolvedValue(unlisten);
    unlisten.mockReset();
  });
  afterEach(() => vi.restoreAllMocks());
  it("uses the latest handler without registering again", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender, unmount } = renderHook(
      ({ handler }) => useTauriEvent("changed", handler),
      { initialProps: { handler: first } },
    );
    await act(async () => {});
    rerender({ handler: second });
    act(() => emit("value"));
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith("value");
    expect(listen).toHaveBeenCalledTimes(1);
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
  it("ignores late events and cleans a registration completed after unmount", async () => {
    let resolveRegistration!: (off: () => void) => void;
    vi.mocked(listen).mockReturnValueOnce(
      new Promise((resolve) => {
        resolveRegistration = resolve;
      }),
    );
    const handler = vi.fn();
    const { unmount } = renderHook(() => useTauriEvent("changed", handler));
    unmount();
    act(() => emit("late"));
    await act(async () => resolveRegistration(unlisten));
    expect(handler).not.toHaveBeenCalled();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
  it("ignores an old subscription after switching events", async () => {
    const handler = vi.fn();
    const { rerender } = renderHook(
      ({ name }) => useTauriEvent(name, handler),
      { initialProps: { name: "first" } },
    );
    await act(async () => {});
    rerender({ name: "second" });
    act(() => emit("old"));
    act(() => emit("new", 1));
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith("new");
  });
  it.each([false, true])(
    "reports handler failures (async=%s)",
    async (asynchronous) => {
      const error = new Error("handler failed");
      const log = vi.spyOn(console, "error").mockImplementation(() => {});
      const handler = asynchronous
        ? () => Promise.reject(error)
        : () => {
            throw error;
          };
      renderHook(() => useTauriEvent("changed", handler));
      await act(async () => {
        expect(() => emit("value")).not.toThrow();
      });
      expect(log).toHaveBeenCalledWith("Failed to handle changed event", error);
    },
  );
  it("reports subscription failures", async () => {
    const error = new Error("subscription failed");
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(listen).mockRejectedValueOnce(error);
    renderHook(() => useTauriEvent("changed", vi.fn()));
    await act(async () => {});
    expect(log).toHaveBeenCalledWith(
      "Failed to subscribe changed event",
      error,
    );
  });
  it("reports cleanup failures without throwing out of unmount", async () => {
    const error = new Error("window closed");
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    unlisten.mockImplementation(() => {
      throw error;
    });
    const { unmount } = renderHook(() => useTauriEvent("changed", vi.fn()));
    await act(async () => {});
    expect(() => unmount()).not.toThrow();
    expect(log).toHaveBeenCalledWith(
      "Failed to unsubscribe changed event",
      error,
    );
  });
  it.each([false, true])(
    "handles rejected unlisten promises (late registration=%s)",
    async (late) => {
      const error = new Error("IPC closed");
      const log = vi.spyOn(console, "error").mockImplementation(() => {});
      unlisten.mockRejectedValue(error);
      let finish!: (off: () => void) => void;
      if (late)
        vi.mocked(listen).mockReturnValueOnce(
          new Promise((resolve) => {
            finish = resolve;
          }),
        );
      const { unmount } = renderHook(() => useTauriEvent("changed", vi.fn()));
      await act(async () => {});
      await act(async () => {
        unmount();
        if (late) finish(unlisten);
      });
      expect(log).toHaveBeenCalledWith(
        "Failed to unsubscribe changed event",
        error,
      );
    },
  );
});
