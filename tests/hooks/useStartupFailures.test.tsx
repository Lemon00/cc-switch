import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStartupFailures } from "@/hooks/useStartupFailures";
const m = vi.hoisted(() => ({ listen: vi.fn(), read: vi.fn(), off: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: m.listen }));
vi.mock("@/lib/api/proxy", () => ({
  proxyApi: { takeStartupAttachFailures: m.read },
}));
const failure = { appType: "codex", stack: false, error: "port busy" };
let notify: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  m.read.mockResolvedValue([]);
  m.listen.mockImplementation(async (_: string, listener: () => void) => {
    notify = listener;
    return m.off;
  });
});
describe("startup routing failures", () => {
  it("reads failures when restoration finished before the window subscribed", async () => {
    m.read.mockResolvedValue([failure]);
    const { result } = renderHook(() => useStartupFailures());
    await waitFor(() => expect(result.current[0]).toEqual([failure]));
  });
  it("retains a late failure when the earlier empty response arrives last", async () => {
    let initial!: (value: unknown[]) => void;
    m.read.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          initial = resolve;
        }),
    );
    const { result } = renderHook(() => useStartupFailures());
    await waitFor(() => expect(m.read).toHaveBeenCalledOnce());
    m.read.mockResolvedValue([failure]);
    act(() => notify());
    await waitFor(() => expect(result.current[0]).toEqual([failure]));
    await act(async () => initial([]));
    expect(result.current[0]).toEqual([failure]);
  });
  it("does not restore a dismissed failure on duplicate completion events", async () => {
    m.read.mockResolvedValue([failure]);
    const { result } = renderHook(() => useStartupFailures());
    await waitFor(() => expect(result.current[0]).toEqual([failure]));
    act(() => result.current[1]([]));
    await act(async () => notify());
    expect(result.current[0]).toEqual([]);
    expect(m.read).toHaveBeenCalledTimes(2);
  });
  it("cleans up a subscription that resolves after unmount", async () => {
    let subscribe!: (off: () => void) => void;
    m.listen.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          subscribe = resolve;
        }),
    );
    const { unmount } = renderHook(() => useStartupFailures());
    unmount();
    await act(async () => subscribe(m.off));
    expect(m.off).toHaveBeenCalledOnce();
    expect(m.read).not.toHaveBeenCalled();
  });
});
