import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { usePanelState } from "@/tray-panel/usePanelState";
import { trayPanelApi, type TrayPanelState } from "@/tray-panel/api";
const snapshot = (feedback: string): TrayPanelState => ({
  feedback,
  apps: [],
  problems: [],
  lightweight: false,
  quitHint: null,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
afterEach(() => vi.restoreAllMocks());
describe("tray panel snapshot reads", () => {
  it("coalesces notifications during a read and never publishes its stale snapshot", async () => {
    const old = deferred<TrayPanelState>();
    const fresh = deferred<TrayPanelState>();
    const read = vi
      .spyOn(trayPanelApi, "state")
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(fresh.promise);
    const { result } = renderHook(() => usePanelState());
    let completion!: Promise<void>;
    act(() => {
      completion = result.current.load();
      void result.current.load();
      void result.current.load();
    });
    expect(read).toHaveBeenCalledTimes(1);
    await act(async () => {
      old.resolve(snapshot("old"));
    });
    expect(read).toHaveBeenCalledTimes(2);
    expect(result.current.state).toBeNull();
    await act(async () => {
      fresh.resolve(snapshot("new"));
      await completion;
    });
    expect(result.current.state?.feedback).toBe("new");
    act(() => result.current.clearFeedback());
    expect(result.current.state?.feedback).toBeNull();
  });
  it("recovers after failed reads and retains the last good snapshot", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const read = vi
      .spyOn(trayPanelApi, "state")
      .mockResolvedValueOnce(snapshot("cached"))
      .mockRejectedValueOnce(new Error("temporary database failure"))
      .mockResolvedValueOnce(snapshot("recovered"));
    const { result } = renderHook(() => usePanelState());
    await act(() => result.current.load());
    await act(() => result.current.load());
    expect(result.current.state?.feedback).toBe("cached");
    await act(() => result.current.load());
    expect(result.current.state?.feedback).toBe("recovered");
    expect(read).toHaveBeenCalledTimes(3);
  });
  it("does not enqueue another read or publish data after unmount", async () => {
    const pending = deferred<TrayPanelState>();
    const read = vi
      .spyOn(trayPanelApi, "state")
      .mockReturnValue(pending.promise);
    const { result, unmount } = renderHook(() => usePanelState());
    act(() => {
      void result.current.load();
      void result.current.load();
    });
    const lastState = result.current.hasSnapshot;
    unmount();
    pending.resolve(snapshot("late"));
    await waitFor(() => expect(read).toHaveBeenCalledTimes(1));
    expect(lastState.current).toBe("");
  });
});
