import { describe, expect, it } from "vitest";
import { resolveProviderIcon } from "./providerIcon";
describe("resolveProviderIcon", () => {
  it("does not reinterpret another app's provider icon", () => {
    expect(resolveProviderIcon("codex", "grok", "")).toBe("grok");
    expect(resolveProviderIcon("dsh", "deepseek")).toBe("deepseek");
    expect(resolveProviderIcon("claude", " custom-provider ", "#fff")).toBe(
      "custom-provider",
    );
  });
  it("keeps the name fallback available when no icon is configured", () => {
    expect(resolveProviderIcon("claude")).toBeUndefined();
    expect(resolveProviderIcon("codex", "  ")).toBeUndefined();
  });
});
