import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  readAuthOpenBrowser,
  useAuthOpenBrowser,
  writeAuthOpenBrowser,
} from "@/lib/authLoginLink";
describe("authLoginLink", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    act(() => writeAuthOpenBrowser(true));
    window.localStorage.removeItem("cc-switch.authCenter.openBrowser");
  });
  it("keeps the selected browser preference when localStorage rejects writes", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { result } = renderHook(() => useAuthOpenBrowser());
    act(() => writeAuthOpenBrowser(false));
    expect(result.current).toBe(false);
    expect(readAuthOpenBrowser()).toBe(false);
  });
  it("opens the browser by default and remembers when it is turned off", () => {
    expect(readAuthOpenBrowser()).toBe(true);
    const { result } = renderHook(() => useAuthOpenBrowser());
    expect(result.current).toBe(true);
    act(() => writeAuthOpenBrowser(false));
    expect(result.current).toBe(false);
    expect(readAuthOpenBrowser()).toBe(false);
    act(() => writeAuthOpenBrowser(true));
    expect(result.current).toBe(true);
    expect(
      window.localStorage.getItem("cc-switch.authCenter.openBrowser"),
    ).toBeNull();
  });
});
