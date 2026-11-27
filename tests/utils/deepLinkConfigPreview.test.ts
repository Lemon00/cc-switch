import { describe, expect, it } from "vitest";
import { parseDeepLinkConfigPreview } from "@/utils/deepLinkConfigPreview";
const encodeBase64 = (value: string) =>
  btoa(String.fromCharCode(...new TextEncoder().encode(value)));
describe("parseDeepLinkConfigPreview", () => {
  it("also masks secrets in Codex TOML previews", () => {
    const preview = parseDeepLinkConfigPreview({
      app: "codex",
      config: encodeBase64(
        JSON.stringify({
          auth: { OPENAI_API_KEY: "secret-auth-key" },
          config: 'experimental_bearer_token = "secret-config-key"',
        }),
      ),
      configFormat: "json",
    });
    expect(preview?.type).toBe("codex");
    expect(preview?.tomlConfig).not.toContain("secret-config-key");
  });
});
it("previews agy wrapped and flat configs without unrelated native options", () => {
  for (const config of [
    {
      env: {
        GEMINI_API_KEY: "fixture",
        GEMINI_MODEL: "legacy",
        NODE_OPTIONS: "ignored",
      },
      config: { model: { name: "native" }, arbitrary: "ignored" },
    },
    {
      GEMINI_API_KEY: "fixture",
      GEMINI_MODEL: "native",
      NODE_OPTIONS: "ignored",
    },
  ]) {
    const preview = parseDeepLinkConfigPreview({
      app: "agy",
      config: encodeBase64(JSON.stringify(config)),
    });
    expect(preview?.type).toBe("agy");
    expect(preview?.model).toBe("native");
    expect(preview?.env).toEqual({ GEMINI_API_KEY: "fixture" });
    expect(JSON.stringify(preview)).not.toContain("ignored");
  }
});
