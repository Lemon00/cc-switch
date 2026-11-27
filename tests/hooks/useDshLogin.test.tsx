import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDshLogin } from "@/components/providers/DshAccount";
const mocks = vi.hoisted(() => ({
  start: vi.fn(),
  poll: vi.fn(),
  cancel: vi.fn(),
  link: vi.fn(),
}));
vi.mock("@/lib/api/dsh", () => ({
  dshApi: {
    loginStart: (...args: unknown[]) => mocks.start(...args),
    loginPoll: (...args: unknown[]) => mocks.poll(...args),
    loginCancel: (...args: unknown[]) => mocks.cancel(...args),
  },
}));
vi.mock("@/lib/authLoginLink", () => ({
  deliverLoginLink: (...args: unknown[]) => mocks.link(...args),
  readAuthOpenBrowser: () => true,
}));
const flow = {
  flowId: "one",
  authUrl: "https://example.com/authorize",
  expiresIn: 600,
};
describe("useDshLogin", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.start.mockReset().mockResolvedValue(flow);
    mocks.poll.mockReset().mockResolvedValue(false);
    mocks.cancel.mockReset().mockResolvedValue(undefined);
    mocks.link.mockReset().mockResolvedValue({ linkOnly: false });
  });
  afterEach(() => vi.useRealTimers());
  it.each(["cancel", "unmount"])(
    "cancels a delayed start after %s without opening the browser",
    async (ending) => {
      let resolve!: (value: typeof flow) => void;
      mocks.start.mockImplementation(
        () =>
          new Promise((done) => {
            resolve = done;
          }),
      );
      const { result, unmount } = renderHook(() => useDshLogin(vi.fn()));
      let starting!: Promise<void>;
      act(() => {
        starting = result.current.start();
      });
      if (ending === "unmount") unmount();
      else act(() => result.current.login.onCancel());
      await act(async () => {
        resolve(flow);
        await starting;
      });
      expect(mocks.cancel).toHaveBeenCalledWith("one");
      expect(mocks.link).not.toHaveBeenCalled();
      if (ending !== "unmount") unmount();
    },
  );
  it("does not overlap polls or let an old error overwrite a replacement flow", async () => {
    let reject!: (error: Error) => void;
    mocks.poll.mockImplementationOnce(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    );
    const onSignedIn = vi.fn();
    const { result, unmount } = renderHook(() => useDshLogin(onSignedIn));
    await act(async () => {
      await result.current.start();
    });
    await act(() => vi.advanceTimersByTimeAsync(6000));
    expect(mocks.poll).toHaveBeenCalledTimes(1);
    mocks.start.mockResolvedValue({ ...flow, flowId: "two" });
    await act(async () => {
      await result.current.start();
    });
    expect(mocks.cancel).toHaveBeenCalledWith("one");
    await act(async () => reject(new Error("obsolete failure")));
    expect(result.current.login.error).toBeNull();
    expect(result.current.login.polling).toBe(true);
    mocks.poll.mockResolvedValue(true);
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(onSignedIn).toHaveBeenCalledTimes(1);
    unmount();
  });
});
