import { describe, it, expect } from "vitest";
import {
  detectCodingPlanProvider,
  extractBaseUrlForUsageDetection,
  injectCodingPlanUsageScript,
} from "./codingPlanProviders";
describe("coding plan URL boundaries", () => {
  it.each([
    "https://evil.example/api.kimi.com/coding",
    "https://evil.example/?url=https://api.z.ai/v1",
    "https://bigmodel.cn.evil.example/v1",
    "https://api.z.ai@evil.example/v1",
    "https://zenmux.ai.evil.example/usage",
    "https://evil.example/api.commandcode.ai/provider",
    "https://opencode.ai/zen/gopher",
    "https://api.kimi.com/coding-other",
    "https://ark.cn-beijing.volces.com/api/plans",
    "ftp://api.z.ai/v1",
  ])("does not infer a plan from an unrelated URL: %s", (baseUrl) => {
    expect(detectCodingPlanProvider(baseUrl)).toBeNull();
  });
  it.each([
    ["api.kimi.com/coding/v1", "kimi"],
    ["https://open.bigmodel.cn:8443/api/paas/v4", "zhipu"],
    ["https://zenmux.ai/api/v1", "zenmux"],
    ["https://ark.cn-beijing.volces.com/api/plan/v3", "volcengine"],
    ["https://API.CommandCode.ai./provider?x=1", "command_code"],
  ])("recognizes parsed URLs: %s", (baseUrl, expected) => {
    expect(detectCodingPlanProvider(baseUrl)).toBe(expected);
  });
});
// codex 预设的 config 由 generateThirdPartyConfig 生成，这里取其等效形态
const OPENCODE_GO_CODEX_TOML = `model_provider = "custom"
model = "glm-5.2"
model_reasoning_effort = "high"

[model_providers.custom]
name = "opencode_go"
base_url = "https://opencode.ai/zen/go/v1"
wire_api = "responses"
requires_openai_auth = true`;
describe("detectCodingPlanProvider (OpenCode Go)", () => {
  it("matches both base variants across apps", () => {
    // Claude 使用 /inference/go/anthropic，Codex/OpenCode 使用
    // /inference/go/v1；旧入口 /zen/go[/v1] 仍可用，同样命中
    expect(
      detectCodingPlanProvider("https://opencode.ai/inference/go/anthropic"),
    ).toBe("opencode_go");
    expect(
      detectCodingPlanProvider("https://opencode.ai/inference/go/v1"),
    ).toBe("opencode_go");
    expect(detectCodingPlanProvider("https://opencode.ai/zen/go")).toBe(
      "opencode_go",
    );
    expect(detectCodingPlanProvider("https://opencode.ai/zen/go/v1")).toBe(
      "opencode_go",
    );
  });
  it("does not match OpenCode Zen (pay-as-you-go, no usage API)", () => {
    expect(detectCodingPlanProvider("https://opencode.ai/zen/v1")).toBeNull();
    expect(
      detectCodingPlanProvider("https://opencode.ai/inference/v1"),
    ).toBeNull();
  });
});
describe("detectCodingPlanProvider (MiniMax)", () => {
  it.each([
    "https://api.minimax.cn/v1",
    "https://api.minimaxi.com/v1",
    "https://api.minimax.io/v1",
    "https://API.MINIMAX.CN/anthropic",
  ])("recognizes the MiniMax usage provider for %s", (baseUrl) => {
    expect(detectCodingPlanProvider(baseUrl)).toBe("minimax");
  });
  it.each([
    "https://api.minimax.cn.example.com/v1",
    "https://proxy.example.com/api.minimax.io/v1",
  ])("ignores look-alike MiniMax hosts such as %s", (baseUrl) => {
    expect(detectCodingPlanProvider(baseUrl)).toBeNull();
  });
});
describe("extractBaseUrlForUsageDetection", () => {
  it("reads env.ANTHROPIC_BASE_URL for claude", () => {
    const config = {
      env: { ANTHROPIC_BASE_URL: "https://opencode.ai/zen/go" },
    };
    expect(extractBaseUrlForUsageDetection("claude", config)).toBe(
      "https://opencode.ai/zen/go",
    );
  });
  it("reads base_url from the codex TOML config", () => {
    expect(
      extractBaseUrlForUsageDetection("codex", {
        auth: { OPENAI_API_KEY: "" },
        config: OPENCODE_GO_CODEX_TOML,
      }),
    ).toBe("https://opencode.ai/zen/go/v1");
  });
  it.each(["opencode", "dsh"])("reads options.baseURL for %s", (app) => {
    expect(
      extractBaseUrlForUsageDetection(app, {
        options: { baseURL: "https://opencode.ai/zen/go/v1" },
      }),
    ).toBe("https://opencode.ai/zen/go/v1");
  });
  it("reads Antigravity's Google base URL field", () => {
    const kimi = "https://api.kimi.com/coding/v1";
    expect(
      extractBaseUrlForUsageDetection("agy", {
        env: { GOOGLE_GEMINI_BASE_URL: kimi },
      }),
    ).toBe(kimi);
  });
  it("returns null for unknown apps", () => {
    expect(
      extractBaseUrlForUsageDetection("unknown", { baseUrl: "https://x" }),
    ).toBeNull();
  });
  it.each([
    "claude-desktop",
    "gemini",
    "grokbuild",
    "hermes",
    "openclaw",
    "pi",
    "omp",
    "mcode",
    "kimi",
  ])("does not reactivate the retired %s client", (app) => {
    expect(
      extractBaseUrlForUsageDetection(app, {
        env: {
          ANTHROPIC_BASE_URL: "https://api.kimi.com/coding/v1",
          GOOGLE_GEMINI_BASE_URL: "https://api.kimi.com/coding/v1",
        },
        options: { baseURL: "https://api.kimi.com/coding/v1" },
        config: OPENCODE_GO_CODEX_TOML,
      }),
    ).toBeNull();
  });
  it.each(["claude", "codex", "opencode", "dsh", "agy"])(
    "ignores missing or malformed settings for %s",
    (app) => {
      expect(extractBaseUrlForUsageDetection(app, undefined)).toBeNull();
      expect(extractBaseUrlForUsageDetection(app, {})).toBeNull();
      expect(
        extractBaseUrlForUsageDetection(app, {
          env: { ANTHROPIC_BASE_URL: 42, GOOGLE_GEMINI_BASE_URL: false },
          options: { baseURL: {} },
          config: 42,
        }),
      ).toBeNull();
    },
  );
});
type TestProvider = {
  settingsConfig?: Record<string, any>;
  meta?: Record<string, any>;
};
describe("injectCodingPlanUsageScript", () => {
  const inject = (appId: string, provider: TestProvider) =>
    injectCodingPlanUsageScript(appId, provider);
  const expectInjected = (provider: TestProvider) => {
    expect(provider.meta?.usage_script).toMatchObject({
      enabled: true,
      templateType: "token_plan",
      codingPlanProvider: "opencode_go",
    });
  };
  it("injects OpenCode Go for every app that ships its preset", () => {
    expectInjected(
      inject("claude", {
        settingsConfig: {
          env: { ANTHROPIC_BASE_URL: "https://opencode.ai/zen/go" },
        },
      }),
    );
    expectInjected(
      inject("agy", {
        settingsConfig: {
          env: { GOOGLE_GEMINI_BASE_URL: "https://opencode.ai/zen/go" },
        },
      }),
    );
    expectInjected(
      inject("codex", {
        settingsConfig: { config: OPENCODE_GO_CODEX_TOML },
      }),
    );
    expectInjected(
      inject("opencode", {
        settingsConfig: {
          options: { baseURL: "https://opencode.ai/zen/go/v1" },
        },
      }),
    );
    expectInjected(
      inject("dsh", {
        settingsConfig: {
          options: { baseURL: "https://opencode.ai/zen/go/v1" },
        },
      }),
    );
  });
  it("keeps the existing claude behavior for other coding plans", () => {
    const injected = inject("claude", {
      settingsConfig: {
        env: { ANTHROPIC_BASE_URL: "https://api.kimi.com/coding/v1" },
      },
    });
    expect(injected.meta?.usage_script?.codingPlanProvider).toBe("kimi");
  });
  it("injects every coding plan for non-claude apps", () => {
    expect(
      inject("opencode", {
        settingsConfig: {
          options: { baseURL: "https://open.bigmodel.cn/api/anthropic" },
        },
      }).meta?.usage_script,
    ).toMatchObject({ enabled: true, codingPlanProvider: "zhipu" });
    expect(
      inject("dsh", {
        settingsConfig: {
          options: { baseURL: "https://api.kimi.com/coding/v1" },
        },
      }).meta?.usage_script,
    ).toMatchObject({ enabled: true, codingPlanProvider: "kimi" });
  });
  it("keeps ZenMux claude-only", () => {
    const settingsConfig = { options: { baseURL: "https://zenmux.ai/api/v1" } };
    expect(
      inject("opencode", { settingsConfig }).meta?.usage_script,
    ).toBeUndefined();
  });
  it("preselects Volcengine without enabling it (needs AccessKey first)", () => {
    expect(
      inject("opencode", {
        settingsConfig: {
          options: { baseURL: "https://ark.cn-beijing.volces.com/api/plan/v3" },
        },
      }).meta?.usage_script,
    ).toMatchObject({
      enabled: false,
      templateType: "token_plan",
      codingPlanProvider: "volcengine",
    });
    expect(
      inject("claude", {
        settingsConfig: {
          env: {
            ANTHROPIC_BASE_URL: "https://ark.cn-beijing.volces.com/api/coding",
          },
        },
      }).meta?.usage_script,
    ).toMatchObject({ enabled: false, codingPlanProvider: "volcengine" });
  });
  it("never overwrites an existing usage_script", () => {
    const provider: TestProvider = {
      settingsConfig: {
        env: { ANTHROPIC_BASE_URL: "https://opencode.ai/zen/go" },
      },
      meta: { usage_script: { enabled: false, templateType: "custom" } },
    };
    expect(inject("claude", provider).meta?.usage_script).toEqual({
      enabled: false,
      templateType: "custom",
    });
  });
});
