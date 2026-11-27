import { describe, expect, it } from "vitest";
import { providerPresets } from "./claudeProviderPresets";
import { codexProviderPresets } from "./codexProviderPresets";
import { opencodeProviderPresets } from "./opencodeProviderPresets";
import { getIcon, getIconMetadata } from "../icons/extracted";
const ppioPresetCollections = [
  ["Claude Code", providerPresets],
  ["Codex", codexProviderPresets],
  ["OpenCode", opencodeProviderPresets],
] as const;
const ppioModelId = "deepseek/deepseek-v4.1-flash";
const ppioModelName = "DeepSeek V4.1 Flash";
const ppioAnthropicEndpoint = "https://api.ppio.com/anthropic";
const ppioOpenAiEndpoint = "https://api.ppio.com/openai/v1";
const ppioChatCompletionsEndpoint = `${ppioOpenAiEndpoint}/chat/completions`;
const ppioModelsEndpoint = `${ppioOpenAiEndpoint}/models`;
const ppioBrandFields = {
  websiteUrl: "https://ppio.com",
  apiKeyUrl: "https://ppio.com/activity/ccswitch",
  category: "aggregator",
  isPartner: true,
  partnerPromotionKey: "ppio",
  icon: "ppio",
  iconColor: "#2874FF",
};
function getPpioPreset<
  T extends {
    name: string;
  },
>(presets: readonly T[]) {
  return presets.find((preset) => preset.name === "PPIO");
}
describe("PPIO provider presets", () => {
  it.each(ppioPresetCollections)(
    "%s registers exactly one PPIO preset",
    (_name, presets) => {
      expect(presets.filter((preset) => preset.name === "PPIO")).toHaveLength(
        1,
      );
    },
  );
  it("configures Claude Code with the native Anthropic endpoint", () => {
    const claude = getPpioPreset(providerPresets)!;
    expect(claude).toMatchObject({
      ...ppioBrandFields,
      settingsConfig: {
        env: {
          ANTHROPIC_BASE_URL: ppioAnthropicEndpoint,
          ANTHROPIC_AUTH_TOKEN: "",
          ANTHROPIC_MODEL: `${ppioModelId}[1m]`,
          ANTHROPIC_DEFAULT_HAIKU_MODEL: `${ppioModelId}[1m]`,
          ANTHROPIC_DEFAULT_SONNET_MODEL: `${ppioModelId}[1m]`,
          ANTHROPIC_DEFAULT_OPUS_MODEL: `${ppioModelId}[1m]`,
        },
      },
      endpointCandidates: [ppioAnthropicEndpoint],
      modelsUrl: ppioModelsEndpoint,
    });
  });
  it("configures Codex for OpenAI Chat translation and reasoning", () => {
    const codex = getPpioPreset(codexProviderPresets)!;
    expect(codex).toMatchObject({
      ...ppioBrandFields,
      auth: { OPENAI_API_KEY: "" },
      endpointCandidates: [ppioOpenAiEndpoint],
      apiFormat: "openai_chat",
      modelCatalog: [
        {
          model: ppioModelId,
          displayName: ppioModelName,
          contextWindow: 1048576,
          inputModalities: ["text"],
        },
      ],
      codexChatReasoning: {
        supportsThinking: true,
        supportsEffort: false,
        thinkingParam: "thinking",
        effortParam: "none",
        outputFormat: "reasoning_content",
      },
    });
    expect(codex.config).toContain(`model = "${ppioModelId}"`);
    expect(codex.config).toContain(`base_url = "${ppioOpenAiEndpoint}"`);
    expect(codex.config).toContain('wire_api = "responses"');
  });
  it("configures OpenCode with a versioned OpenAI-compatible base", () => {
    const opencode = getPpioPreset(opencodeProviderPresets)!;
    expect(opencode).toMatchObject({
      ...ppioBrandFields,
      settingsConfig: {
        npm: "@ai-sdk/openai-compatible",
        name: "PPIO",
        options: {
          baseURL: ppioOpenAiEndpoint,
          apiKey: "",
          setCacheKey: true,
        },
        models: { [ppioModelId]: { name: ppioModelName } },
      },
      templateValues: {
        apiKey: { label: "API Key", placeholder: "", editorValue: "" },
      },
    });
    expect(Object.keys(opencode.settingsConfig.models)).toEqual([
      ppioModelId,
      "moonshotai/kimi-k3.1-pro",
      "zai-org/glm-5.3",
    ]);
    expect(`${opencode.settingsConfig.options.baseURL}/chat/completions`).toBe(
      ppioChatCompletionsEndpoint,
    );
  });
  it("registers the PPIO brand icon", () => {
    expect(getIcon("ppio")).toContain("<title>PPIO</title>");
    expect(getIconMetadata("ppio")).toMatchObject({
      displayName: "PPIO",
      defaultColor: "#2874FF",
    });
  });
});
