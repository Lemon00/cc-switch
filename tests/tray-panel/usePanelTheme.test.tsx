import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { usePanelTheme } from "@/tray-panel/usePanelTheme";

afterEach(() => {
  localStorage.removeItem("cc-switch-theme");
  vi.restoreAllMocks();
});

it("returns to the system theme when another window clears storage", () => {
  vi.spyOn(window, "matchMedia").mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  } as unknown as MediaQueryList);
  localStorage.setItem("cc-switch-theme", "dark");
  const { result } = renderHook(() => usePanelTheme(1));
  expect(result.current).toEqual({ dark: true, explicit: true });
  act(() => {
    localStorage.clear();
    window.dispatchEvent(new StorageEvent("storage", { key: null }));
  });
  expect(result.current).toEqual({ dark: false, explicit: null });
  expect(document.documentElement).toHaveClass("light");
  expect(document.documentElement).not.toHaveClass("dark");
});
