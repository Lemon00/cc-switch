import { type PropsWithChildren } from "react";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateProvider, useUpdate } from "@/contexts/UpdateContext";
import { checkForUpdate } from "@/lib/updater";

vi.mock("@/lib/updater", () => ({ checkForUpdate: vi.fn() }));

const dismissedKey = "ccswitch:update:dismissedVersion";
const legacyKey = "dismissedUpdateVersion";
const available = (version = "2.0.0") => ({
  status: "available" as const,
  info: { currentVersion: "1.0.0", availableVersion: version },
});
const wrapper = ({ children }: PropsWithChildren) => (
  <UpdateProvider>{children}</UpdateProvider>
);

describe("UpdateContext", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: vi.fn((key: string) => values.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => {
        values.set(key, value);
      }),
      removeItem: vi.fn((key: string) => {
        values.delete(key);
      }),
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(checkForUpdate).mockReset().mockResolvedValue(available());
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("does not turn storage read failure into update failure", async () => {
    vi.mocked(localStorage.getItem).mockImplementation(() => {
      throw new Error("storage denied");
    });
    const { result } = renderHook(useUpdate, { wrapper });
    await act(async () => {
      expect(await result.current.checkUpdate()).toBe(true);
    });
    expect(result.current.hasUpdate).toBe(true);
    expect(result.current.error).toBeNull();
    expect(result.current.isDismissed).toBe(false);
  });

  it("keeps dismiss and reset functional in memory when writes fail", async () => {
    for (const operation of [localStorage.setItem, localStorage.removeItem]) {
      vi.mocked(operation).mockImplementation(() => {
        throw new Error("storage full");
      });
    }
    const { result } = renderHook(useUpdate, { wrapper });
    await act(async () => {
      await result.current.checkUpdate();
    });
    act(() => result.current.dismissUpdate());
    expect(result.current.isDismissed).toBe(true);
    await act(async () => {
      await result.current.checkUpdate();
    });
    expect(result.current.isDismissed).toBe(true);
    act(() => result.current.resetDismiss());
    expect(result.current.isDismissed).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it("migrates the legacy preference and does not dismiss a newer version", async () => {
    localStorage.setItem(legacyKey, "2.0.0");
    const { result } = renderHook(useUpdate, { wrapper });
    await act(async () => {
      await result.current.checkUpdate();
    });
    expect(result.current.isDismissed).toBe(true);
    expect(localStorage.getItem(dismissedKey)).toBe("2.0.0");
    expect(localStorage.getItem(legacyKey)).toBeNull();
    vi.mocked(checkForUpdate).mockResolvedValue(available("3.0.0"));
    await act(async () => {
      await result.current.checkUpdate();
    });
    expect(result.current.isDismissed).toBe(false);
    act(() => result.current.dismissUpdate());
    expect(localStorage.getItem(dismissedKey)).toBe("3.0.0");
    act(() => result.current.resetDismiss());
    expect(localStorage.getItem(dismissedKey)).toBeNull();
  });

  it("retains a readable legacy preference even if migration cannot persist", async () => {
    localStorage.setItem(legacyKey, "2.0.0");
    vi.mocked(localStorage.setItem).mockImplementation(() => {
      throw new Error("full");
    });
    const { result } = renderHook(useUpdate, { wrapper });
    await act(async () => {
      await result.current.checkUpdate();
    });
    expect(result.current.isDismissed).toBe(true);
    expect(localStorage.getItem(legacyKey)).toBe("2.0.0");
  });

  it("preserves real update errors and permits a later retry", async () => {
    const failure = new Error("network unavailable");
    vi.mocked(checkForUpdate).mockRejectedValueOnce(failure);
    const { result } = renderHook(useUpdate, { wrapper });
    await act(async () => {
      await expect(result.current.checkUpdate()).rejects.toBe(failure);
    });
    expect(result.current.error).toBe("network unavailable");
    expect(result.current.isChecking).toBe(false);
    await act(async () => {
      await result.current.checkUpdate();
    });
    expect(result.current.hasUpdate).toBe(true);
    expect(result.current.error).toBeNull();
  });
});
