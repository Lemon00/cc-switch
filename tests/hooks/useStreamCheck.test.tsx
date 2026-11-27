import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStreamCheck } from "@/hooks/useStreamCheck";
import type { AppId } from "@/lib/api";
import type { StreamCheckResult } from "@/lib/api/connectivity-check";
const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/api/connectivity-check", () => ({
  streamCheckProvider: mocks.check,
}));
vi.mock("@/lib/toast", () => ({ toast: mocks }));
const reachable: StreamCheckResult = {
  status: "operational",
  success: true,
  message: "ok",
  testedAt: 0,
  retryCount: 0,
};
function deferred() {
  let resolve!: (result: StreamCheckResult) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<StreamCheckResult>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
beforeEach(() => Object.values(mocks).forEach((mock) => mock.mockReset()));
describe("useStreamCheck", () => {
  it("coalesces duplicate checks and clears state only after completion", async () => {
    const request = deferred();
    mocks.check.mockReturnValue(request.promise);
    const { result } = renderHook(() => useStreamCheck("claude"));
    let first!: Promise<StreamCheckResult | null>;
    let second!: Promise<StreamCheckResult | null>;
    await act(async () => {
      first = result.current.checkProvider("same", "One");
      second = result.current.checkProvider("same", "One");
    });
    expect(first).toBe(second);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(result.current.isChecking("same")).toBe(true);
    await act(async () => {
      request.resolve(reachable);
      await first;
    });
    expect(result.current.isChecking("same")).toBe(false);
    expect(mocks.success).toHaveBeenCalledTimes(1);
  });
  it("isolates application changes and silences obsolete or unmounted requests", async () => {
    const oldRequest = deferred();
    const newRequest = deferred();
    mocks.check
      .mockReturnValueOnce(oldRequest.promise)
      .mockReturnValueOnce(newRequest.promise);
    const view = renderHook(
      ({ appId }: { appId: AppId }) => useStreamCheck(appId),
      { initialProps: { appId: "claude" as AppId } },
    );
    let oldResult!: Promise<StreamCheckResult | null>;
    let newResult!: Promise<StreamCheckResult | null>;
    await act(async () => {
      oldResult = view.result.current.checkProvider("same", "One");
    });
    view.rerender({ appId: "codex" });
    await act(async () => {
      newResult = view.result.current.checkProvider("same", "Two");
    });
    await act(async () => {
      oldRequest.resolve(reachable);
      await oldResult;
    });
    expect(view.result.current.isChecking("same")).toBe(true);
    expect(mocks.success).not.toHaveBeenCalled();
    view.unmount();
    newRequest.reject(new Error("late"));
    await newResult;
    expect(mocks.error).not.toHaveBeenCalled();
  });
  it("allows retry after failure", async () => {
    mocks.check
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(reachable);
    const { result } = renderHook(() => useStreamCheck("claude"));
    await act(async () => {
      expect(await result.current.checkProvider("same", "One")).toBeNull();
    });
    expect(result.current.isChecking("same")).toBe(false);
    await act(async () => {
      expect(await result.current.checkProvider("same", "One")).toEqual(
        reachable,
      );
    });
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });
});
