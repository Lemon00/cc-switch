import { afterEach, describe, expect, it, vi } from "vitest";
import {
  parseView,
  readStoredApp,
  readStoredView,
  storeApp,
  storeView,
} from "@/lib/navigation";
afterEach(() => vi.restoreAllMocks());
describe("navigation persistence", () => {
  it("uses safe defaults when storage reads are denied", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readStoredApp()).toBe("claude");
    expect(readStoredView()).toBe("providers");
  });
  it("does not interrupt navigation when storage writes fail", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => storeApp("codex")).not.toThrow();
    expect(() => storeView("mcp")).not.toThrow();
  });
  it("rejects prototype names and retired views while preserving supported views", () => {
    for (const value of ["toString", "__proto__", "constructor", "invalid"]) {
      expect(parseView(value)).toBeNull();
    }
    expect(parseView("openclawEnv")).toBeNull();
    expect(parseView("providers")).toBe("providers");
    localStorage.setItem("cc-switch-last-view", "openclawEnv");
    expect(readStoredView()).toBe("providers");
  });
});
