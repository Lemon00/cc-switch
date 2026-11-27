import type { DshProviderConfig, ProviderCategory } from "@/types";
import type { PresetTheme } from "./claudeProviderPresets";
import type { PresetFamilyFields } from "./presetFamilies";
export interface DshProviderPreset extends PresetFamilyFields {
  theme?: PresetTheme;
  name: string;
  nameKey?: string;
  providerKey: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: DshProviderConfig;
  category?: ProviderCategory;
  isPartner?: boolean;
  primePartner?: boolean;
  partnerPromotionKey?: string;
  icon?: string;
  iconColor?: string;
  endpointCandidates?: string[];
}
const kimiK31ProModels = (): DshProviderConfig["models"] => ({
  "kimi-k3.1-pro": {
    name: "Kimi K3.1 Pro",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1048576,
      output: 131072,
    },
    compat: {
      supportsStore: false,
      supportsDeveloperRole: false,
      supportsReasoningEffort: true,
      maxTokensField: "max_tokens",
      supportsStrictMode: false,
      thinkingFormat: "openai",
      requiresReasoningContentOnAssistantMessages: true,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      high: "high",
      max: "max",
    },
  },
  "kimi-k3.1-pro-highspeed": {
    name: "Kimi K3.1 Pro HighSpeed",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1048576,
      output: 131072,
    },
    compat: {
      supportsStore: false,
      supportsDeveloperRole: false,
      supportsReasoningEffort: true,
      maxTokensField: "max_tokens",
      supportsStrictMode: false,
      thinkingFormat: "openai",
      requiresReasoningContentOnAssistantMessages: true,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      high: "high",
      max: "max",
    },
  },
  "kimi-k3": {
    name: "Kimi K3",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1048576,
      output: 131072,
    },
    compat: {
      supportsStore: false,
      supportsDeveloperRole: false,
      supportsReasoningEffort: true,
      maxTokensField: "max_tokens",
      supportsStrictMode: false,
      thinkingFormat: "openai",
      requiresReasoningContentOnAssistantMessages: true,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      high: "high",
      max: "max",
    },
  },
  "kimi-k2.7-code": {
    name: "Kimi K2.7 Code",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 262144,
      output: 262144,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "kimi-k2.7-code-highspeed": {
    name: "Kimi K2.7 Code HighSpeed",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 262144,
      output: 262144,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "kimi-k2.6": {
    name: "Kimi K2.6",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 262144,
      output: 262144,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const kimiForCodingModels = (): DshProviderConfig["models"] => ({
  "kimi-for-coding": {
    name: "Kimi For Coding",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 262144,
      output: 32768,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const claudeOpus55Models = (): DshProviderConfig["models"] => ({
  "claude-opus-5-5": {
    name: "Claude Opus 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "claude-sonnet-5-5": {
    name: "Claude Sonnet 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "claude-fable-5-1": {
    name: "Claude Fable 5.1",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
});

const gpt61SolModels = (): DshProviderConfig["models"] => ({
  "gpt-6.1-sol": {
    name: "GPT-6.1 Sol",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "gpt-6-astra": {
    name: "GPT-6 Astra",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const claudeOpus55Models2 = (): DshProviderConfig["models"] => ({
  "claude-opus-5-5": {
    name: "Claude Opus 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "claude-sonnet-5-5": {
    name: "Claude Sonnet 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const claudeOpus55Models3 = (): DshProviderConfig["models"] => ({
  "claude-opus-5-5": {
    name: "Claude Opus 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "claude-sonnet-5-5": {
    name: "Claude Sonnet 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "claude-haiku-5-5": {
    name: "Claude Haiku 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "claude-fable-5-1": {
    name: "Claude Fable 5.1",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
});

const anthropicClaudeOpus55Models = (): DshProviderConfig["models"] => ({
  "anthropic/claude-opus-5.5": {
    name: "Claude Opus 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "anthropic/claude-sonnet-5.5": {
    name: "Claude Sonnet 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "anthropic/claude-fable-5.1": {
    name: "Claude Fable 5.1",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
});

const arkCodeLatestModels = (): DshProviderConfig["models"] => ({
  "ark-code-latest": {
    name: "Ark Code Latest",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 128000,
      output: 16384,
    },
    reasoningEfforts: false,
  },
});

const claudeOpus55Models4 = (): DshProviderConfig["models"] => ({
  "claude-opus-5-5": {
    name: "Claude Opus 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    compat: {
      forceAdaptiveThinking: true,
    },
    reasoning: true,
    reasoningEfforts: {
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "claude-sonnet-5-5": {
    name: "Claude Sonnet 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "claude-haiku-5-5": {
    name: "Claude Haiku 5.5",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const gpt61SolModels2 = (): DshProviderConfig["models"] => ({
  "gpt-6.1-sol": {
    name: "GPT-6.1 Sol",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: "none",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "gpt-6-sol": {
    name: "GPT-6 Sol",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: "none",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "gpt-6-astra": {
    name: "GPT-6 Astra",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
});

const gpt61SolModels3 = (): DshProviderConfig["models"] => ({
  "gpt-6.1-sol": {
    name: "GPT-6.1 Sol",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: "none",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "gpt-6-astra": {
    name: "GPT-6 Astra",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "gpt-6-luna": {
    name: "GPT-6 Luna",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 272000,
      output: 128000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: "none",
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
});

const glm53Models = (): DshProviderConfig["models"] => ({
  "glm-5.3": {
    name: "GLM-5.3",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 1048576,
      output: 131072,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "glm-5.3-flash": {
    name: "GLM-5.3 Flash",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1048576,
      output: 131072,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const qwen38MaxModels = (): DshProviderConfig["models"] => ({
  "qwen3.8-max": {
    name: "Qwen3.8 Max",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 983616,
      output: 131072,
    },
    compat: {
      thinkingFormat: "qwen",
      supportsDeveloperRole: false,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "qwen3.8-flash": {
    name: "Qwen3.8 Flash",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 983616,
      output: 131072,
    },
    compat: {
      thinkingFormat: "qwen",
      supportsDeveloperRole: false,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "qwen3.7-max": {
    name: "Qwen3.7 Max",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 131072,
    },
    compat: {
      thinkingFormat: "qwen",
      supportsDeveloperRole: false,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const step35Flash2603Models = (): DshProviderConfig["models"] => ({
  "step-3.5-flash-2603": {
    name: "Step 3.5 Flash 2603",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 256000,
      output: 256000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "step-3.5-flash": {
    name: "Step 3.5 Flash",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 256000,
      output: 256000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "step-3.7-flash": {
    name: "Step 3.7 Flash",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 256000,
      output: 256000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
  "step-5-preview": {
    name: "Step 5 Preview",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 64000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const katCoderProV25Models = (): DshProviderConfig["models"] => ({
  "kat-coder-pro-v2.5": {
    name: "KAT-Coder Pro V2.5",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 256000,
      output: 80000,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const MiniMaxM31FlashModels = (): DshProviderConfig["models"] => ({
  "MiniMax-M3.1-Flash": {
    name: "MiniMax-M3.1-Flash",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 131072,
    },
    reasoning: true,
    reasoningEfforts: {
      low: "low",
      medium: "medium",
      high: "high",
      xhigh: "xhigh",
      max: "max",
    },
  },
  "MiniMax-M3": {
    name: "MiniMax-M3",
    modalities: {
      input: ["text", "image"],
      output: ["text"],
    },
    limit: {
      context: 1000000,
      output: 131072,
    },
    reasoning: true,
    reasoningEfforts: {
      off: null,
      minimal: "minimal",
      low: "low",
      medium: "medium",
      high: "high",
    },
  },
});

const autoModels = (): DshProviderConfig["models"] => ({
  auto: {
    name: "Auto",
    modalities: {
      input: ["text"],
      output: ["text"],
    },
    limit: {
      context: 196608,
      output: 32768,
    },
    reasoningEfforts: false,
  },
});

export const dshProviderPresets: DshProviderPreset[] = [
  {
    name: "Kimi",
    family: "kimi",
    planKey: "payg",
    regionKey: "cn",
    providerKey: "cc-switch-kimi",
    websiteUrl:
      "https://platform.kimi.com?track_id=track-7cf2b91dcde043eda6ef9a95951a042c&aff=cc-switch",
    apiKeyUrl: "https://platform.kimi.com/console/api-keys?aff=cc-switch",
    settingsConfig: {
      name: "Kimi",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.moonshot.cn/v1",
        apiKey: "",
      },
      models: kimiK31ProModels(),
    },
    category: "cn_official",
    primePartner: true,
    partnerPromotionKey: "kimi",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi Global",
    family: "kimi",
    planKey: "payg",
    regionKey: "intl",
    providerKey: "cc-switch-kimi-global",
    websiteUrl:
      "https://platform.kimi.ai?track_id=track-674ed6e2af924a5682a87421f7cf753a&aff=cc-switch",
    apiKeyUrl: "https://platform.kimi.ai/console/api-keys?aff=cc-switch",
    settingsConfig: {
      name: "Kimi Global",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.moonshot.ai/v1",
        apiKey: "",
      },
      models: kimiK31ProModels(),
    },
    category: "cn_official",
    partnerPromotionKey: "kimi",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi For Coding",
    family: "kimi",
    planKey: "coding",
    regionKey: "cn",
    providerKey: "cc-switch-kimi-for-coding",
    websiteUrl: "https://www.kimi.com/code/?aff=cc-switch",
    apiKeyUrl: "https://platform.kimi.com/console/api-keys?aff=cc-switch",
    settingsConfig: {
      name: "Kimi For Coding",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.kimi.com/coding",
        apiKey: "",
      },
      models: kimiForCodingModels(),
    },
    category: "cn_official",
    primePartner: true,
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi For Coding Global",
    family: "kimi",
    planKey: "coding",
    regionKey: "intl",
    providerKey: "cc-switch-kimi-for-coding-global",
    websiteUrl: "https://www.kimi.ai/code?aff=cc-switch",
    apiKeyUrl: "https://www.kimi.ai/code?aff=cc-switch",
    settingsConfig: {
      name: "Kimi For Coding Global",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.kimi.ai/coding",
        apiKey: "",
      },
      models: kimiForCodingModels(),
    },
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "PackyCode",
    providerKey: "cc-switch-packy-code",
    websiteUrl: "https://www.packyapi.ai",
    apiKeyUrl: "https://www.packyapi.ai/register?aff=cc-switch",
    settingsConfig: {
      name: "PackyCode",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://www.packyapi.ai",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "packycode",
    icon: "packycode",
  },
  {
    name: "APINebula",
    providerKey: "cc-switch-apinebula",
    websiteUrl: "https://apinebula.ai",
    apiKeyUrl: "https://apinebula.ai/VjM74M",
    settingsConfig: {
      name: "APINebula",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://apinebula.ai/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "apinebula",
    icon: "apinebula",
  },
  {
    name: "AICodeMirror",
    providerKey: "cc-switch-aicode-mirror",
    websiteUrl: "https://www.aicodemirror.ai",
    apiKeyUrl: "https://www.aicodemirror.ai/register?invitecode=9915W3",
    settingsConfig: {
      name: "AICodeMirror",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.aicodemirror.ai/api/claudecode",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "aicodemirror",
    icon: "aicodemirror",
    iconColor: "#000000",
  },
  {
    name: "hao.ai",
    providerKey: "cc-switch-hao-ai",
    websiteUrl: "https://hao.ai",
    apiKeyUrl: "https://hao.ai/invite/DTBZS3OR",
    settingsConfig: {
      name: "hao.ai",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.hao.ai/anthropic",
        apiKey: "",
      },
      models: {
        "anthropic/claude-opus-5-5": {
          name: "Claude Opus 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            forceAdaptiveThinking: true,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "anthropic/claude-sonnet-5-5": {
          name: "Claude Sonnet 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "haoai",
    icon: "haoai",
  },
  {
    name: "FennoAI",
    providerKey: "cc-switch-fenno-ai",
    websiteUrl: "https://api.fenno.ai",
    apiKeyUrl:
      "https://api.fenno.ai/register?redirect=/purchase?tab=subscription%26group=16&aff=P9MR3D3PLCNL",
    settingsConfig: {
      name: "FennoAI",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.fenno.ai/v1",
        apiKey: "",
      },
      models: {
        "gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "fenno",
    icon: "fenno",
  },
  {
    name: "RunAPI",
    providerKey: "cc-switch-run-api",
    websiteUrl: "https://runapi.co",
    apiKeyUrl: "https://runapi.co/register?aff=iOKB",
    settingsConfig: {
      name: "RunAPI",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://runapi.co",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "runapi",
    icon: "runapi",
  },
  {
    name: "OpenModel",
    providerKey: "cc-switch-open-model",
    websiteUrl: "https://www.openmodel.ai",
    apiKeyUrl: "https://www.openmodel.ai?ref=cc-switch",
    settingsConfig: {
      name: "OpenModel",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.openmodel.ai",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "openmodel",
    icon: "openmodel",
  },
  {
    name: "GrooRoute Pro",
    providerKey: "cc-switch-grooroute-pro",
    websiteUrl: "https://grooroute.pro",
    apiKeyUrl: "https://grooroute.pro/sign-up?aff=qqm9",
    settingsConfig: {
      name: "GrooRoute Pro",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://grooroute.pro",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "grooroute",
    icon: "grooroute",
  },
  {
    name: "BuyToken",
    providerKey: "cc-switch-buytoken",
    websiteUrl: "https://buytoken.work",
    apiKeyUrl: "https://buytoken.work/sign-up?aff=cc-switch",
    settingsConfig: {
      name: "BuyToken",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.buytoken.work",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "buytoken",
    icon: "buytoken",
  },
  {
    name: "V-API",
    providerKey: "cc-switch-vapi",
    websiteUrl: "https://gpt.ge",
    apiKeyUrl: "https://gpt.ge/signup?aff=URNP",
    settingsConfig: {
      name: "V-API",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.gpt.ge",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "vapi",
    icon: "vapi",
  },
  {
    name: "TokenFactory",
    nameKey: "providerForm.presets.tokenfactory",
    providerKey: "cc-switch-tokenfactory",
    websiteUrl: "https://tokenfactory.cn",
    apiKeyUrl: "https://console.tokenfactory.cn/login",
    settingsConfig: {
      name: "TokenFactory",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.tokenfactory.cn/v1",
        apiKey: "",
      },
      models: {
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash-1017": {
          name: "DeepSeek V4.1 Flash 1017",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3.8-flash": {
          name: "Qwen3.8 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "tokenfactory",
  },
  {
    name: "FluxA Token Plan",
    providerKey: "cc-switch-fluxa-token-plan",
    websiteUrl: "https://agentmarket.fluxapay.xyz/",
    apiKeyUrl: "https://agentmarket.fluxapay.xyz/marketplace/tokenplans",
    settingsConfig: {
      name: "FluxA Token Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.baiduqianfan.ai/v2/tokenplan/team",
        apiKey: "",
      },
      models: {
        "deepseek-v4-flash-0731": {
          name: "DeepSeek V4 Flash 0731",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v3.2": {
          name: "DeepSeek V3.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 32768,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.1": {
          name: "GLM-5.1",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5": {
          name: "GLM-5",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "fluxa",
    icon: "fluxa",
  },
  {
    name: "AIGoCode",
    providerKey: "cc-switch-aigo-code",
    websiteUrl: "https://aigocode.net",
    apiKeyUrl: "https://aigocode.net/invite/CC-SWITCH",
    settingsConfig: {
      name: "AIGoCode",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.aigocode.dev",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "aigocode",
    icon: "aigocode",
    iconColor: "#5B7FFF",
  },
  {
    name: "Qiniu",
    nameKey: "providerForm.presets.qiniu",
    providerKey: "cc-switch-qiniu",
    websiteUrl: "https://s.qiniu.com/nMvAvy",
    apiKeyUrl: "https://s.qiniu.com/nMvAvy",
    settingsConfig: {
      name: "Qiniu",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.qnaigc.com/v1",
        apiKey: "",
      },
      models: {
        "gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "z-ai/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "z-ai/glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "qiniu",
    icon: "qiniu",
  },
  {
    name: "AICoding",
    providerKey: "cc-switch-aicoding",
    websiteUrl: "https://aicoding.inc",
    apiKeyUrl: "https://aicoding.inc/i/CCSWITCH",
    settingsConfig: {
      name: "AICoding",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.aicoding.inc",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "aicoding",
    icon: "aicoding",
    iconColor: "#000000",
  },
  {
    name: "SubRouter",
    providerKey: "cc-switch-sub-router",
    websiteUrl: "https://subrouter.ai",
    apiKeyUrl: "https://subrouter.ai/register?aff=l3ri",
    settingsConfig: {
      name: "SubRouter",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://subrouter.ai/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "subrouter",
    icon: "subrouter",
  },
  {
    name: "OspreyAI",
    providerKey: "cc-switch-ospreyai",
    websiteUrl: "https://open.ospreyai.cn",
    apiKeyUrl: "https://open.ospreyai.cn",
    settingsConfig: {
      name: "OspreyAI",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://open.ospreyai.cn",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "ospreyai",
    icon: "ospreyai",
  },
  {
    name: "Shengsuanyun",
    nameKey: "providerForm.presets.shengsuanyun",
    providerKey: "cc-switch-shengsuanyun",
    websiteUrl: "https://www.shengsuanyun.com/?from=CH_4HHXMRYF",
    apiKeyUrl: "https://www.shengsuanyun.com/?from=CH_4HHXMRYF",
    settingsConfig: {
      name: "Shengsuanyun",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://router.shengsuanyun.com/api",
        apiKey: "",
      },
      models: anthropicClaudeOpus55Models(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "shengsuanyun",
    icon: "shengsuanyun",
  },
  {
    name: "88API",
    providerKey: "cc-switch-88api",
    websiteUrl: "https://88api.ai",
    apiKeyUrl: "https://88api.ai/sign-up?aff=HSGY",
    settingsConfig: {
      name: "88API",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.88api.ai",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "88api",
    icon: "88api",
  },
  {
    name: "APIKEY.FUN",
    providerKey: "cc-switch-apikey-fun",
    websiteUrl: "https://apikey.fan",
    apiKeyUrl: "https://apikey.fan/register?aff=CCSwitch",
    settingsConfig: {
      name: "APIKEY.FUN",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.apikey.fan",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "apikeyfun",
    icon: "apikeyfun",
  },
  {
    name: "9527CODE",
    providerKey: "cc-switch-9527-code",
    websiteUrl: "https://9527.codes",
    apiKeyUrl: "https://9527.codes/register?aff=e5zI",
    settingsConfig: {
      name: "9527CODE",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://9527.codes",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "9527code",
    icon: "9527code",
  },
  {
    name: "Code0",
    providerKey: "cc-switch-code0",
    websiteUrl: "https://code0.ai",
    apiKeyUrl: "https://code0.ai/agent/register/B2XHxGjGmRvqgznY",
    settingsConfig: {
      name: "Code0",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://code0.ai/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "aggregator",
    icon: "code0",
  },
  {
    name: "TeamoRouter",
    providerKey: "cc-switch-teamo-router",
    websiteUrl: "https://teamorouter.com",
    apiKeyUrl:
      "https://teamorouter.com/?utm_source=cc_switch&utm_medium=referral&utm_campaign=ai_directory",
    settingsConfig: {
      name: "TeamoRouter",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.teamorouter.com/v1",
        apiKey: "",
      },
      models: {
        "gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-5.6-sol": {
          name: "GPT-5.6 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "teamorouter",
    icon: "teamorouter",
  },
  {
    name: "TeamoRouter Claude",
    providerKey: "cc-switch-teamo-router-claude",
    websiteUrl: "https://teamorouter.com",
    apiKeyUrl:
      "https://teamorouter.com/?utm_source=cc_switch&utm_medium=referral&utm_campaign=ai_directory",
    settingsConfig: {
      name: "TeamoRouter Claude",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.teamorouter.com",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "teamorouter",
    icon: "teamorouter",
  },
  {
    name: "PPIO",
    providerKey: "cc-switch-ppio",
    websiteUrl: "https://ppio.com",
    apiKeyUrl: "https://ppio.com/activity/ccswitch",
    settingsConfig: {
      name: "PPIO",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.ppio.com/openai/v1",
        apiKey: "",
      },
      models: {
        "deepseek/deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 393216,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "moonshotai/kimi-k3.1-pro": {
          name: "Kimi K3.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "ppio",
    icon: "ppio",
    iconColor: "#2874FF",
  },
  {
    name: "ClaudeCN",
    providerKey: "cc-switch-claude-cn",
    websiteUrl: "https://claudecn.top",
    apiKeyUrl: "https://claudecn.ai/register?aff=HEL9",
    settingsConfig: {
      name: "ClaudeCN",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://claudecn.top",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "claudecn",
    icon: "claudecn",
  },
  {
    name: "火山 Agent Plan",
    family: "volcengine",
    planKey: "agentPlan",
    providerKey: "cc-switch-agentplan",
    websiteUrl:
      "https://www.volcengine.com/activity/agentplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_source=OWO&utm_medium=devrel-1&utm_campaign=hw&utm_term=ccswitch&utm_content=hw",
    apiKeyUrl:
      "https://www.volcengine.com/activity/agentplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_source=OWO&utm_medium=devrel-1&utm_campaign=hw&utm_term=ccswitch&utm_content=hw",
    settingsConfig: {
      name: "火山 Agent Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ark.cn-beijing.volces.com/api/plan/v3",
        apiKey: "",
      },
      models: arkCodeLatestModels(),
    },
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "volcengine_agentplan",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "火山 Coding Plan",
    family: "volcengine",
    planKey: "codingPlan",
    providerKey: "cc-switch-codingplan",
    websiteUrl:
      "https://www.volcengine.com/activity/codingplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://www.volcengine.com/activity/codingplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    settingsConfig: {
      name: "火山 Coding Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ark.cn-beijing.volces.com/api/coding/v3",
        apiKey: "",
      },
      models: arkCodeLatestModels(),
    },
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "volcengine_codingplan",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "BytePlus",
    providerKey: "cc-switch-byte-plus",
    websiteUrl:
      "https://www.byteplus.com/en/product/modelark?utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://www.byteplus.com/en/product/modelark?utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    settingsConfig: {
      name: "BytePlus",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ark.ap-southeast.bytepluses.com/api/coding/v3",
        apiKey: "",
      },
      models: arkCodeLatestModels(),
    },
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "byteplus",
    icon: "byteplus",
    iconColor: "#3370FF",
  },
  {
    name: "Volcengine Doubao",
    family: "volcengine",
    planKey: "payg",
    nameKey: "providerForm.presets.doubaoseed",
    providerKey: "cc-switch-dou-bao-seed",
    websiteUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    settingsConfig: {
      name: "Volcengine Doubao",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ark.cn-beijing.volces.com/api/v3",
        apiKey: "",
      },
      models: {
        "doubao-seed-2-1-pro-260915": {
          name: "Doubao Seed 2.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 128000,
            output: 16384,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "doubaoseed",
    icon: "doubao",
    iconColor: "#3370FF",
  },
  {
    name: "A6API",
    providerKey: "cc-switch-a6-api",
    websiteUrl: "https://www.a6api.vip",
    apiKeyUrl: "https://a6api.vip/register?aff=AqNr",
    settingsConfig: {
      name: "A6API",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.a6api.vip/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "a6api",
    icon: "a6api",
  },
  {
    name: "CCSub",
    providerKey: "cc-switch-ccsub",
    websiteUrl: "https://www.ccsub.net",
    apiKeyUrl: "https://www.ccsub.net/register?ref=Y6Z8DXEA",
    settingsConfig: {
      name: "CCSub",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://www.ccsub.net/v1",
        apiKey: "",
      },
      models: {
        "gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-6-luna": {
          name: "GPT-6 Luna",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "ccsub",
    icon: "ccsub",
  },
  {
    name: "SSSAiCode",
    providerKey: "cc-switch-sssai-code",
    websiteUrl: "https://sssaicodeapi.com",
    apiKeyUrl: "https://sssaicodeapi.com/register?ref=DCP0SM",
    settingsConfig: {
      name: "SSSAiCode",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://node-hk.sssaicodeapi.com/api",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "sssaicode",
    icon: "sssaicode",
    iconColor: "#000000",
  },
  {
    name: "SoleAPI",
    providerKey: "cc-switch-sole-api",
    websiteUrl: "https://soleapi.com",
    apiKeyUrl: "https://soleapi.com/r/ccswitch",
    settingsConfig: {
      name: "SoleAPI",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://soleapi.com",
        apiKey: "",
      },
      models: claudeOpus55Models4(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "soleapi",
    icon: "soleapi",
  },
  {
    name: "Micu",
    providerKey: "cc-switch-micu",
    websiteUrl: "https://www.micuapi.ai",
    apiKeyUrl: "https://www.micuapi.ai/register?aff=aOYQ",
    settingsConfig: {
      name: "Micu",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://www.micuapi.ai",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "micu",
    icon: "micu",
    iconColor: "#000000",
  },
  {
    name: "RightCode",
    providerKey: "cc-switch-right-code",
    websiteUrl: "https://www.rightapi.ai",
    apiKeyUrl: "https://www.rightapi.ai/register?aff=CCSWITCH",
    settingsConfig: {
      name: "RightCode",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://www.rightapi.ai/codex/v1",
        apiKey: "",
      },
      models: {
        "gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: "none",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
      },
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "rightcode",
    icon: "rc",
    iconColor: "#E96B2C",
  },
  {
    name: "Cubence",
    providerKey: "cc-switch-cubence",
    websiteUrl: "https://cubence.com",
    apiKeyUrl: "https://cubence.com/signup?code=CCSWITCH&source=ccs",
    settingsConfig: {
      name: "Cubence",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.cubence.com",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "cubence",
    icon: "cubence",
    iconColor: "#000000",
  },
  {
    name: "CrazyRouter",
    providerKey: "cc-switch-crazy-router",
    websiteUrl: "https://www.crazyrouter.com",
    apiKeyUrl: "https://www.crazyrouter.com/register?aff=OZcm&ref=cc-switch",
    settingsConfig: {
      name: "CrazyRouter",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://cn.crazyrouter.com",
        apiKey: "",
      },
      models: claudeOpus55Models(),
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "crazyrouter",
    icon: "crazyrouter",
    iconColor: "#000000",
  },
  {
    name: "DMXAPI",
    providerKey: "cc-switch-dmxapi",
    websiteUrl: "https://www.dmxapi.cn",
    apiKeyUrl: "https://www.dmxapi.cn",
    settingsConfig: {
      name: "DMXAPI",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://www.dmxapi.cn",
        apiKey: "",
      },
      models: claudeOpus55Models2(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "dmxapi",
    icon: "dmxapi",
  },
  {
    name: "SudoCode.chat",
    family: "sudocode",
    providerKey: "cc-switch-sudo-code-chat",
    websiteUrl: "https://sudocode.chat",
    apiKeyUrl:
      "https://sudocode.chat/sign-up?aff=CC-SWITCH&utm_source=cc-switch&utm_medium=sponsor&utm_campaign=ccswitch",
    settingsConfig: {
      name: "SudoCode.chat",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://api.sudocode.chat/v1",
        apiKey: "",
      },
      models: {
        "gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: "none",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "gpt-6-sol": {
          name: "GPT-6 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: "none",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "gpt-6-luna": {
          name: "GPT-6 Luna",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: "none",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
      },
    },
    category: "third_party",
    isPartner: true,
    partnerPromotionKey: "sudocode",
    icon: "sudocode",
  },
  {
    name: "SudoCode.us",
    family: "sudocode",
    providerKey: "cc-switch-sudo-code-us",
    websiteUrl: "https://sudocode.us",
    apiKeyUrl: "https://sudocode.us",
    settingsConfig: {
      name: "SudoCode.us",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://sudocode.us/v1",
        apiKey: "",
      },
      models: gpt61SolModels2(),
    },
    category: "third_party",
    isPartner: true,
    icon: "sudocode-us",
  },
  {
    name: "Modelflare",
    providerKey: "cc-switch-modelflare",
    websiteUrl: "https://modelflare.dev",
    apiKeyUrl:
      "https://modelflare.dev/sign-up?utm_medium=referral&utm_source=ccswitch",
    settingsConfig: {
      name: "Modelflare",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://modelflare.dev/v1",
        apiKey: "",
      },
      models: gpt61SolModels3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "modelflare",
    icon: "modelflare",
  },
  {
    name: "AllTokens",
    providerKey: "cc-switch-alltokens",
    websiteUrl: "https://alltokens.cc",
    apiKeyUrl: "https://alltokens.cc/register",
    settingsConfig: {
      name: "AllTokens",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://alltokens.cc/v1",
        apiKey: "",
      },
      models: gpt61SolModels3(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "alltokens",
    icon: "alltokens",
  },
  {
    name: "AllTokens SI",
    providerKey: "cc-switch-alltokens-si",
    websiteUrl: "https://alltokens.si",
    apiKeyUrl: "https://alltokens.si/enterprise-register",
    settingsConfig: {
      name: "AllTokens SI",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://alltokens.si/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "alltokens_si",
    icon: "alltokens-si",
  },
  {
    name: "Amux",
    providerKey: "cc-switch-amux",
    websiteUrl: "https://amux.ai",
    apiKeyUrl: "https://amux.ai",
    settingsConfig: {
      name: "Amux",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.amux.ai/v1",
        apiKey: "",
      },
      models: gpt61SolModels(),
    },
    category: "aggregator",
    icon: "amux",
  },
  {
    name: "AtlasCloud",
    providerKey: "cc-switch-atlas-cloud",
    websiteUrl: "https://www.atlascloud.ai/console/coding-plan",
    apiKeyUrl: "https://www.atlascloud.ai/console/coding-plan",
    settingsConfig: {
      name: "AtlasCloud",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.atlascloud.ai/v1",
        apiKey: "",
      },
      models: {
        "zai-org/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "atlascloud",
  },
  {
    name: "Soshow",
    providerKey: "cc-switch-soshow",
    websiteUrl: "https://aimarket.so-show.com",
    apiKeyUrl: "https://aimarket.so-show.com/workbench/access-key",
    settingsConfig: {
      name: "Soshow",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://maas.so-show.com",
        apiKey: "",
      },
      models: {
        "claude-sonnet-5": {
          name: "Claude Sonnet 5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            forceAdaptiveThinking: true,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "claude-opus-5": {
          name: "Claude Opus 5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            forceAdaptiveThinking: true,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
      },
    },
    category: "aggregator",
    icon: "soshow",
  },
  {
    name: "DeepSeek",
    providerKey: "cc-switch-deep-seek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    settingsConfig: {
      name: "DeepSeek",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.deepseek.com/v1",
        apiKey: "",
      },
      models: {
        "deepseek-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
      },
    },
    category: "cn_official",
    icon: "deepseek",
    iconColor: "#1E88E5",
  },
  {
    name: "Zhipu GLM",
    family: "zhipu",
    regionKey: "cn",
    providerKey: "cc-switch-zhipu-glm",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/claude-code",
    settingsConfig: {
      name: "Zhipu GLM",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://open.bigmodel.cn/api/coding/paas/v4",
        apiKey: "",
      },
      models: glm53Models(),
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Zhipu GLM en",
    family: "zhipu",
    regionKey: "intl",
    providerKey: "cc-switch-zhipu-glm-en",
    websiteUrl: "https://z.ai",
    apiKeyUrl: "https://z.ai/subscribe",
    settingsConfig: {
      name: "Zhipu GLM en",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.z.ai/api/coding/paas/v4",
        apiKey: "",
      },
      models: glm53Models(),
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "千问AI平台",
    family: "qianwen",
    planKey: "payg",
    providerKey: "cc-switch-qianwenai",
    websiteUrl: "https://platform.qianwenai.com/?utm_content=g_20000002971",
    apiKeyUrl:
      "https://platform.qianwenai.com/home/api-keys?utm_content=g_20000002972",
    settingsConfig: {
      name: "千问AI平台",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://maas.qianwenaiapi.com/compatible-mode/v1",
        apiKey: "",
      },
      models: {
        "qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          compat: {
            thinkingFormat: "qwen",
            supportsDeveloperRole: false,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  {
    name: "千问AI平台 Token Plan",
    family: "qianwen",
    planKey: "tokenPlan",
    providerKey: "cc-switch-qianwenai-token-plan",
    websiteUrl:
      "https://platform.qianwenai.com/pricing/token-plan?utm_content=g_20000002977",
    apiKeyUrl:
      "https://platform.qianwenai.com/home/api-keys?utm_content=g_20000002978",
    settingsConfig: {
      name: "千问AI平台 Token Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://token-plan.maas.qianwenaiapi.com/compatible-mode/v1",
        apiKey: "",
      },
      models: {
        "qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          compat: {
            thinkingFormat: "qwen",
            supportsDeveloperRole: false,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3.8-flash": {
          name: "Qwen3.8 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          compat: {
            thinkingFormat: "qwen",
            supportsDeveloperRole: false,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "qianwenai",
    iconColor: "#624AFF",
  },
  {
    name: "QwenCloud",
    family: "qwencloud",
    planKey: "payg",
    providerKey: "cc-switch-qwencloud",
    websiteUrl: "https://home.qwencloud.com/?utm_content=g_20000002974",
    apiKeyUrl: "https://home.qwencloud.com/api-keys?utm_content=g_20000002975",
    settingsConfig: {
      name: "QwenCloud",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL:
          "https://ap-southeast-1.maas.qwencloudapi.com/compatible-mode/v1",
        apiKey: "",
      },
      models: qwen38MaxModels(),
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud For Coding",
    family: "qwencloud",
    planKey: "coding",
    providerKey: "cc-switch-qwencloud-coding",
    websiteUrl: "https://www.qwencloud.com",
    apiKeyUrl: "https://home.qwencloud.com/api-keys",
    settingsConfig: {
      name: "QwenCloud For Coding",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic",
        apiKey: "",
      },
      models: {
        "qwen3.7-plus": {
          name: "Qwen3.7 Plus",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3-coder-plus": {
          name: "Qwen3 Coder Plus",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 65536,
          },
          reasoningEfforts: false,
        },
      },
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "QwenCloud Token Plan",
    family: "qwencloud",
    planKey: "tokenPlan",
    providerKey: "cc-switch-qwencloud-token-plan",
    websiteUrl:
      "https://www.qwencloud.com/pricing/token-plan?utm_content=g_20000002980",
    apiKeyUrl: "https://home.qwencloud.com/api-keys?utm_content=g_20000002981",
    settingsConfig: {
      name: "QwenCloud Token Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://token-plan.maas.qwencloudapi.com/compatible-mode/v1",
        apiKey: "",
      },
      models: qwen38MaxModels(),
    },
    category: "cn_official",
    icon: "qwencloud",
    iconColor: "#6336E7",
  },
  {
    name: "StepFun",
    family: "stepfun",
    regionKey: "cn",
    providerKey: "cc-switch-step-fun",
    websiteUrl: "https://platform.stepfun.com/step-plan",
    apiKeyUrl: "https://platform.stepfun.com/interface-key",
    settingsConfig: {
      name: "StepFun",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.stepfun.com/step_plan/v1",
        apiKey: "",
      },
      models: step35Flash2603Models(),
    },
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "StepFun en",
    family: "stepfun",
    regionKey: "intl",
    providerKey: "cc-switch-step-fun-en",
    websiteUrl: "https://platform.stepfun.ai/step-plan",
    apiKeyUrl: "https://platform.stepfun.ai/interface-key",
    settingsConfig: {
      name: "StepFun en",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.stepfun.ai/step_plan/v1",
        apiKey: "",
      },
      models: step35Flash2603Models(),
    },
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "StepFun Step Plan",
    providerKey: "cc-switch-step-fun-step-plan",
    websiteUrl: "https://platform.stepfun.com/docs/zh/step-plan/overview",
    apiKeyUrl: "https://platform.stepfun.com/interface-key",
    settingsConfig: {
      name: "StepFun Step Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.stepfun.com/step_plan/v1",
        apiKey: "",
      },
      models: {
        "step-3.5-flash": {
          name: "Step 3.5 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 256000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "step-3.7-flash": {
          name: "Step 3.7 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 256000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "step-5-preview": {
          name: "Step 5 Preview",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 64000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#005AFF",
  },
  {
    name: "ModelScope",
    providerKey: "cc-switch-model-scope",
    websiteUrl: "https://modelscope.cn",
    apiKeyUrl: "https://modelscope.cn/my/myaccesstoken",
    settingsConfig: {
      name: "ModelScope",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api-inference.modelscope.cn/v1",
        apiKey: "",
      },
      models: {
        "ZhipuAI/GLM-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "modelscope",
    iconColor: "#624AFF",
  },
  {
    name: "StreamLake",
    family: "streamlake",
    regionKey: "cn",
    providerKey: "cc-switch-streamlake",
    websiteUrl: "https://www.streamlake.com",
    apiKeyUrl:
      "https://console.streamlake.com/console/wanqing/api-key?channel=ccswitch",
    settingsConfig: {
      name: "StreamLake",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://wanqing.streamlakeapi.com/api/gateway/v1/endpoints",
        apiKey: "",
      },
      models: katCoderProV25Models(),
    },
    category: "cn_official",
    icon: "streamlake",
  },
  {
    name: "StreamLake en",
    family: "streamlake",
    regionKey: "intl",
    providerKey: "cc-switch-kat-coder",
    websiteUrl: "https://console.streamlake.ai",
    apiKeyUrl: "https://console.streamlake.ai/console/api-key?channel=ccswitch",
    settingsConfig: {
      name: "StreamLake en",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://vanchin.streamlake.ai/api/gateway/v1/endpoints",
        apiKey: "",
      },
      models: katCoderProV25Models(),
    },
    category: "cn_official",
    icon: "streamlake",
  },
  {
    name: "Longcat",
    providerKey: "cc-switch-longcat",
    websiteUrl: "https://longcat.chat/platform",
    apiKeyUrl: "https://longcat.chat/platform/api_keys",
    settingsConfig: {
      name: "Longcat",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.longcat.chat/openai/v1",
        apiKey: "",
      },
      models: {
        "LongCat-2.0": {
          name: "LongCat 2.0",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "longcat",
    iconColor: "#29E154",
  },
  {
    name: "MiniMax",
    family: "minimax",
    regionKey: "cn",
    providerKey: "cc-switch-mini-max",
    websiteUrl: "https://platform.minimax.cn",
    apiKeyUrl: "https://platform.minimax.cn/subscribe/token-plan",
    settingsConfig: {
      name: "MiniMax",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.minimax.cn/v1",
        apiKey: "",
      },
      models: MiniMaxM31FlashModels(),
    },
    category: "cn_official",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "MiniMax en",
    family: "minimax",
    regionKey: "intl",
    providerKey: "cc-switch-mini-max-en",
    websiteUrl: "https://platform.minimax.io",
    apiKeyUrl: "https://platform.minimax.io/subscribe/coding-plan",
    settingsConfig: {
      name: "MiniMax en",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.minimax.io/v1",
        apiKey: "",
      },
      models: MiniMaxM31FlashModels(),
    },
    category: "cn_official",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "BaiLing",
    providerKey: "cc-switch-bai-ling",
    websiteUrl: "https://developer.ant-ling.com/zh-CN/docs/",
    apiKeyUrl: "https://chat.ant-ling.com/open",
    settingsConfig: {
      name: "BaiLing",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.ant-ling.com/v1",
        apiKey: "",
      },
      models: {
        "Ling-2.6-1T": {
          name: "Ling 2.6-1T",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 32768,
          },
          reasoningEfforts: false,
        },
      },
    },
    category: "cn_official",
    icon: "bailing",
  },
  {
    name: "Xiaomi MiMo",
    family: "xiaomi-mimo",
    planKey: "payg",
    providerKey: "cc-switch-xiaomi-mi-mo",
    websiteUrl: "https://platform.xiaomimimo.com",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/api-keys",
    settingsConfig: {
      name: "Xiaomi MiMo",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.xiaomimimo.com/v1",
        apiKey: "",
      },
      models: {
        "mimo-v2.6-pro": {
          name: "MiMo V2.6 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "mimo-v2.6-flash": {
          name: "MiMo V2.6 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "mimo-v2.6-pro-ultraspeed": {
          name: "MiMo V2.6 Pro UltraSpeed",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "Xiaomi MiMo Token Plan (China)",
    family: "xiaomi-mimo",
    planKey: "tokenPlan",
    providerKey: "cc-switch-xiaomi-mi-mo-token-plan-china",
    websiteUrl: "https://platform.xiaomimimo.com/#/token-plan",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/plan-manage",
    settingsConfig: {
      name: "Xiaomi MiMo Token Plan (China)",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://token-plan-cn.xiaomimimo.com/v1",
        apiKey: "",
      },
      models: {
        "mimo-v2.6-pro": {
          name: "MiMo V2.6 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "mimo-v2.6-flash": {
          name: "MiMo V2.6 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "OpenCode Go",
    providerKey: "cc-switch-open-code-go",
    websiteUrl: "https://opencode.ai/go",
    apiKeyUrl: "https://opencode.ai/go",
    settingsConfig: {
      name: "OpenCode Go",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://opencode.ai/zen/go/v1",
        apiKey: "",
      },
      models: {
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            high: "high",
            max: "max",
          },
        },
        "glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3.1-pro": {
          name: "Kimi K3.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            requiresReasoningContentOnAssistantMessages: true,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "mimo-v2.6-pro": {
          name: "MiMo V2.6 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3.1-flash": {
          name: "MiniMax-M3.1-Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "third_party",
    partnerPromotionKey: "opencode_go",
    icon: "opencode",
    iconColor: "#211E1E",
  },
  {
    name: "AiHubMix",
    providerKey: "cc-switch-ai-hub-mix",
    websiteUrl: "https://aihubmix.com",
    apiKeyUrl: "https://aihubmix.com",
    settingsConfig: {
      name: "AiHubMix",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://aihubmix.com",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "aihubmix",
    iconColor: "#006FFB",
  },
  {
    name: "CherryIN",
    providerKey: "cc-switch-cherry-in",
    websiteUrl: "https://open.cherryin.ai",
    apiKeyUrl: "https://open.cherryin.ai/console/token",
    settingsConfig: {
      name: "CherryIN",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://open.cherryin.net",
        apiKey: "",
      },
      models: anthropicClaudeOpus55Models(),
    },
    category: "aggregator",
    icon: "cherryin",
  },
  {
    name: "OpenRouter",
    providerKey: "cc-switch-open-router",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    settingsConfig: {
      name: "OpenRouter",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://openrouter.ai/api",
        apiKey: "",
      },
      models: {
        "anthropic/claude-opus-5.5": {
          name: "Claude Opus 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            forceAdaptiveThinking: true,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "anthropic/claude-sonnet-5.5": {
          name: "Claude Sonnet 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic/claude-haiku-5.5": {
          name: "Claude Haiku 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic/claude-fable-5.1": {
          name: "Claude Fable 5.1",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          compat: {
            forceAdaptiveThinking: true,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
      },
    },
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
  },
  {
    name: "TheRouter",
    providerKey: "cc-switch-the-router",
    websiteUrl: "https://therouter.ai",
    apiKeyUrl: "https://dashboard.therouter.ai",
    settingsConfig: {
      name: "TheRouter",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.therouter.ai/v1",
        apiKey: "",
      },
      models: {
        "anthropic/claude-sonnet-5-5": {
          name: "Claude Sonnet 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic/claude-fable-5-1": {
          name: "Claude Fable 5.1",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "openai/gpt-6.1-sol": {
          name: "GPT-6.1 Sol",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "openai/gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 272000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "google/gemini-3.8-flash": {
          name: "Gemini 3.8 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen/qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "therouter",
  },
  {
    name: "Baseten",
    providerKey: "cc-switch-baseten",
    websiteUrl: "https://www.baseten.co",
    apiKeyUrl: "https://app.baseten.co/settings/api_keys",
    settingsConfig: {
      name: "Baseten",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://inference.baseten.co/v1",
        apiKey: "",
      },
      models: {
        "zai-org/GLM-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/Kimi-K3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/GLM-5.3-Flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-ai/DeepSeek-V4.1-Flash-1017": {
          name: "DeepSeek V4.1 Flash 1017",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/GLM-5.3-Fast": {
          name: "GLM-5.3 Fast",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "baseten",
  },
  {
    name: "Novita AI",
    providerKey: "cc-switch-novita-ai",
    websiteUrl: "https://novita.ai",
    apiKeyUrl: "https://novita.ai",
    settingsConfig: {
      name: "Novita AI",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.novita.ai/openai",
        apiKey: "",
      },
      models: {
        "zai-org/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/kimi-k3.1-pro": {
          name: "Kimi K3.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek/deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax/minimax-m3.1-flash": {
          name: "MiniMax-M3.1-Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen/qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "xiaomimimo/mimo-v2.6-pro": {
          name: "MiMo V2.6 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "novita",
    iconColor: "#000000",
  },
  {
    name: "Nvidia",
    providerKey: "cc-switch-nvidia",
    websiteUrl: "https://build.nvidia.com",
    apiKeyUrl: "https://build.nvidia.com/settings/api-keys",
    settingsConfig: {
      name: "Nvidia",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://integrate.api.nvidia.com/v1",
        apiKey: "",
      },
      models: {
        "moonshotai/kimi-k2.6": {
          name: "Kimi K2.6",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "z-ai/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "z-ai/glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "nvidia",
    iconColor: "#000000",
  },
  {
    name: "Ollama Cloud",
    providerKey: "cc-switch-ollama-cloud",
    websiteUrl: "https://ollama.com",
    apiKeyUrl: "https://ollama.com/settings/keys",
    settingsConfig: {
      name: "Ollama Cloud",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ollama.com/v1",
        apiKey: "",
      },
      models: {
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 512000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cloud_provider",
    icon: "ollama",
  },
  {
    name: "Ollama",
    providerKey: "cc-switch-ollama",
    websiteUrl: "https://ollama.com",
    settingsConfig: {
      name: "Ollama",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "http://localhost:11434/v1",
        apiKey: "",
      },
      models: {
        "qwen3.5": {
          name: "Qwen3.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 32768,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-oss:20b": {
          name: "GPT-OSS 20B",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "local",
    icon: "ollama",
  },
  {
    name: "LM Studio",
    providerKey: "cc-switch-lmstudio",
    websiteUrl: "https://lmstudio.ai",
    settingsConfig: {
      name: "LM Studio",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "http://localhost:1234/v1",
        apiKey: "",
      },
      models: {
        "openai/gpt-oss-20b": {
          name: "GPT-OSS 20B",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 32768,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "local",
    icon: "lmstudio",
  },
  {
    name: "vLLM",
    providerKey: "cc-switch-vllm",
    websiteUrl: "https://vllm.ai",
    settingsConfig: {
      name: "vLLM",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "http://localhost:8000/v1",
        apiKey: "",
      },
      models: {
        "Qwen/Qwen3.6-27B": {
          name: "Qwen3.6 27B",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 32768,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "local",
    icon: "vllm",
  },
  {
    name: "Meta Model API",
    providerKey: "cc-switch-meta",
    websiteUrl: "https://dev.meta.ai",
    apiKeyUrl: "https://dev.meta.ai",
    settingsConfig: {
      name: "Meta Model API",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.meta.ai/v1",
        apiKey: "",
      },
      models: {
        "muse-wonder-1.0": {
          name: "Muse Wonder 1.0",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
        "muse-spark-1.3": {
          name: "Muse Spark 1.3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
            xhigh: "xhigh",
            max: "max",
          },
        },
      },
    },
    category: "cn_official",
    icon: "meta",
  },
  {
    name: "DeepInfra",
    providerKey: "cc-switch-deepinfra",
    websiteUrl: "https://deepinfra.com",
    apiKeyUrl: "https://deepinfra.com/dash/api_keys",
    settingsConfig: {
      name: "DeepInfra",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.deepinfra.com/v1/openai",
        apiKey: "",
      },
      models: {
        "deepseek-ai/DeepSeek-V4.1-Flash-1017": {
          name: "DeepSeek V4.1 Flash 1017",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 384000,
          },
          compat: {
            supportsDeveloperRole: false,
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "moonshotai/Kimi-K3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            low: "low",
            high: "high",
            max: "max",
          },
        },
        "zai-org/GLM-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/GLM-5.3-Flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "deepinfra",
  },
  {
    name: "Vercel AI Gateway",
    providerKey: "cc-switch-vercel-ai-gateway",
    websiteUrl: "https://vercel.com/ai-gateway",
    apiKeyUrl:
      "https://vercel.com/d?to=%2F%5Bteam%5D%2F%7E%2Fai-gateway%2Fapi-keys",
    settingsConfig: {
      name: "Vercel AI Gateway",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://ai-gateway.vercel.sh/coding-agent/v1",
        apiKey: "",
      },
      models: {
        "anthropic/claude-opus-5.5": {
          name: "Claude Opus 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic/claude-sonnet-5.5": {
          name: "Claude Sonnet 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic/claude-fable-5.1": {
          name: "Claude Fable 5.1",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "openai/gpt-6-astra": {
          name: "GPT-6 Astra",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1050000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "openai/gpt-6-luna": {
          name: "GPT-6 Luna",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1050000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "google/gemini-3.8-flash": {
          name: "Gemini 3.8 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 65535,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "vercel",
  },
  {
    name: "Nebius Token Factory",
    providerKey: "cc-switch-nebius-token-factory",
    websiteUrl: "https://nebius.com/token-factory",
    apiKeyUrl: "https://tokenfactory.nebius.com/project/api-keys",
    settingsConfig: {
      name: "Nebius Token Factory",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.tokenfactory.nebius.com/v1",
        apiKey: "",
      },
      models: {
        "zai-org/GLM-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1024000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/GLM-5.3-Flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1024000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/Kimi-K3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1024000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-ai/DeepSeek-V4.1-Flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "MiniMaxAI/MiniMax-M3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "MiniMaxAI/MiniMax-M3.1-Flash": {
          name: "MiniMax-M3.1-Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/Kimi-K2.7-Code": {
          name: "Kimi K2.7 Code",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "nebius",
  },
  {
    name: "ClinePass",
    providerKey: "cc-switch-clinepass",
    websiteUrl: "https://docs.cline.bot/getting-started/clinepass",
    apiKeyUrl: "https://app.cline.bot",
    settingsConfig: {
      name: "ClinePass",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.cline.bot/api/v1",
        apiKey: "",
      },
      models: {
        "cline-pass/kimi-k3.1-pro": {
          name: "Kimi K3.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/deepseek-v4-pro": {
          name: "DeepSeek V4 Pro",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/qwen3.7-plus": {
          name: "Qwen3.7 Plus",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/minimax-m3.1-flash": {
          name: "MiniMax-M3.1-Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/mimo-v2.6-pro": {
          name: "MiMo V2.6 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "cline-pass/mimo-v2.6-flash": {
          name: "MiMo V2.6 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "third_party",
    icon: "cline",
  },
  {
    name: "Groq",
    providerKey: "cc-switch-groq",
    websiteUrl: "https://groq.com",
    apiKeyUrl: "https://console.groq.com/keys",
    settingsConfig: {
      name: "Groq",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.groq.com/openai/v1",
        apiKey: "",
      },
      models: {
        "openai/gpt-oss-120b": {
          name: "GPT-OSS 120B",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 65536,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "openai/gpt-oss-20b": {
          name: "GPT-OSS 20B",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 65536,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "groq",
  },
  {
    name: "SambaNova",
    providerKey: "cc-switch-sambanova",
    websiteUrl: "https://sambanova.ai",
    apiKeyUrl: "https://cloud.sambanova.ai/apis",
    settingsConfig: {
      name: "SambaNova",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.sambanova.ai/v1",
        apiKey: "",
      },
      models: {
        "MiniMax-M3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "gpt-oss-120b": {
          name: "GPT-OSS 120B",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 65536,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
          },
          reasoning: true,
          reasoningEfforts: {
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cloud_provider",
    icon: "sambanova",
  },
  {
    name: "DigitalOcean",
    providerKey: "cc-switch-digitalocean",
    websiteUrl: "https://www.digitalocean.com/products/ai-platform",
    apiKeyUrl: "https://cloud.digitalocean.com/model-studio/manage-keys",
    settingsConfig: {
      name: "DigitalOcean",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://inference.do-ai.run/v1",
        apiKey: "",
      },
      models: {
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4.1-flash-1017": {
          name: "DeepSeek V4.1 Flash 1017",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "anthropic-claude-sonnet-5.5": {
          name: "Claude Sonnet 5.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cloud_provider",
    icon: "digitalocean",
  },
  {
    name: "Hugging Face",
    providerKey: "cc-switch-hugging-face",
    websiteUrl: "https://huggingface.co/inference/models",
    apiKeyUrl:
      "https://huggingface.co/settings/tokens/new?ownUserPermissions=inference.serverless.write&tokenType=fineGrained",
    settingsConfig: {
      name: "Hugging Face",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://router.huggingface.co/v1",
        apiKey: "",
      },
      models: {
        "zai-org/GLM-5.3:baseten": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "zai-org/GLM-5.3-Flash:baseten": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "moonshotai/Kimi-K3:baseten": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-ai/DeepSeek-V4.1-Flash-1017:baseten": {
          name: "DeepSeek V4.1 Flash 1017",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "huggingface",
  },
  {
    name: "Microsoft Foundry",
    providerKey: "cc-switch-microsoft-foundry",
    websiteUrl: "https://azure.microsoft.com/en-us/products/ai-foundry",
    apiKeyUrl: "https://ai.azure.com/",
    settingsConfig: {
      name: "Microsoft Foundry",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://YOUR_RESOURCE_NAME.services.ai.azure.com/anthropic",
        apiKey: "",
      },
      models: claudeOpus55Models4(),
    },
    category: "cloud_provider",
    icon: "azure",
    iconColor: "#0078D4",
  },
  {
    name: "PIPELLM",
    providerKey: "cc-switch-pipellm",
    websiteUrl: "https://code.pipellm.ai",
    apiKeyUrl: "https://code.pipellm.ai/login?ref=uvw650za",
    settingsConfig: {
      name: "PIPELLM",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://cc-api.pipellm.ai",
        apiKey: "",
      },
      models: claudeOpus55Models4(),
    },
    category: "aggregator",
    icon: "pipellm",
  },
  {
    name: "AICodeWith",
    providerKey: "cc-switch-aicode-with",
    websiteUrl: "https://aicodewith.ai",
    apiKeyUrl: "https://aicodewith.ai/login?tab=register",
    settingsConfig: {
      name: "AICodeWith",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://api.aicodewith.ai/v1",
        apiKey: "",
      },
      models: gpt61SolModels2(),
    },
    category: "aggregator",
    icon: "aicodewith",
    iconColor: "#3A3B40",
  },
  {
    name: "E-FlowCode",
    providerKey: "cc-switch-e-flow-code",
    websiteUrl: "https://e-flowcode.cc",
    apiKeyUrl: "https://e-flowcode.cc",
    settingsConfig: {
      name: "E-FlowCode",
      kind: "custom",
      enabled: true,
      api: "openai-responses",
      options: {
        baseURL: "https://e-flowcode.cc/v1",
        apiKey: "",
      },
      models: gpt61SolModels2(),
    },
    category: "third_party",
    icon: "eflowcode",
    iconColor: "#000000",
  },
  {
    name: "Tencent Token Plan",
    family: "tencent",
    planKey: "tokenPlan",
    regionKey: "cn",
    providerKey: "cc-switch-tencent-token-plan",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan",
    settingsConfig: {
      name: "Tencent Token Plan",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.lkeap.cloud.tencent.com/plan/v3",
        apiKey: "",
      },
      models: {
        "tc-code-latest": {
          name: "Auto",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 196608,
            output: 32768,
          },
          reasoningEfforts: false,
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "minimax-m2.7": {
          name: "MiniMax-M2.7",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 204800,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5": {
          name: "GLM-5",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.1": {
          name: "GLM-5.1",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        hy3: {
          name: "Hy3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "hy3-preview": {
          name: "Hy3 Preview",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent Token Plan (Intl)",
    family: "tencent",
    planKey: "tokenPlan",
    regionKey: "intl",
    providerKey: "cc-switch-tencent-token-plan-intl",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan",
    settingsConfig: {
      name: "Tencent Token Plan (Intl)",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
        apiKey: "",
      },
      models: {
        auto: {
          name: "Auto",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 196608,
            output: 32768,
          },
          reasoningEfforts: false,
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.6": {
          name: "Kimi K2.6",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent Token Plan Enterprise Pro",
    family: "tencent",
    planKey: "enterprisePro",
    regionKey: "cn",
    providerKey: "cc-switch-tencent-token-plan-enterprise-pro",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    settingsConfig: {
      name: "Tencent Token Plan Enterprise Pro",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub.tencentmaas.com/plan/v3",
        apiKey: "",
      },
      models: {
        auto: {
          name: "Auto",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 196608,
            output: 32768,
          },
          reasoningEfforts: false,
        },
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5": {
          name: "GLM-5",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.1": {
          name: "GLM-5.1",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5-turbo": {
          name: "GLM-5 Turbo",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code": {
          name: "Kimi K2.7 Code",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code-highspeed": {
          name: "Kimi K2.7 Code HighSpeed",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.6": {
          name: "Kimi K2.6",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4-flash": {
          name: "DeepSeek V4 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-0731": {
          name: "DeepSeek V4 Flash 0731 GA",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent Token Plan Enterprise Pro (Intl)",
    family: "tencent",
    planKey: "enterprisePro",
    regionKey: "intl",
    providerKey: "cc-switch-tencent-token-plan-enterprise-pro-intl",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    settingsConfig: {
      name: "Tencent Token Plan Enterprise Pro (Intl)",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
        apiKey: "",
      },
      models: {
        auto: {
          name: "Auto",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 196608,
            output: 32768,
          },
          reasoningEfforts: false,
        },
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code": {
          name: "Kimi K2.7 Code",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code-highspeed": {
          name: "Kimi K2.7 Code HighSpeed",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4-flash": {
          name: "DeepSeek V4 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-0731": {
          name: "DeepSeek V4 Flash 0731 GA",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent Token Plan Enterprise Lite",
    family: "tencent",
    planKey: "enterpriseLite",
    regionKey: "cn",
    providerKey: "cc-switch-tencent-token-plan-enterprise-lite",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/tokenplan-e",
    settingsConfig: {
      name: "Tencent Token Plan Enterprise Lite",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub.tencentmaas.com/plan/v3",
        apiKey: "",
      },
      models: autoModels(),
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent Token Plan Enterprise Lite (Intl)",
    family: "tencent",
    planKey: "enterpriseLite",
    regionKey: "intl",
    providerKey: "cc-switch-tencent-token-plan-enterprise-lite-intl",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/tokenplan-e",
    settingsConfig: {
      name: "Tencent Token Plan Enterprise Lite (Intl)",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub-intl.tencentcloudmaas.com/plan/v3",
        apiKey: "",
      },
      models: autoModels(),
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent TokenHub",
    family: "tencent",
    planKey: "payg",
    regionKey: "cn",
    providerKey: "cc-switch-tencent-tokenhub",
    websiteUrl: "https://cloud.tencent.com/product/tokenhub",
    apiKeyUrl: "https://console.cloud.tencent.com/tokenhub/apikey",
    settingsConfig: {
      name: "Tencent TokenHub",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub.tencentmaas.com/v1",
        apiKey: "",
      },
      models: {
        "hy4-preview": {
          name: "Hy4 Preview",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        hy3: {
          name: "Hy3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek/deepseek-v4-flash-vision-exp": {
          name: "DeepSeek V4 Flash Vision Exp",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-0731": {
          name: "DeepSeek V4 Flash 0731 GA",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash": {
          name: "DeepSeek V4 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.1": {
          name: "GLM-5.1",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5v-turbo": {
          name: "GLM-5V Turbo",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5-turbo": {
          name: "GLM-5 Turbo",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5": {
          name: "GLM-5",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code-highspeed": {
          name: "Kimi K2.7 Code HighSpeed",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code": {
          name: "Kimi K2.7 Code",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.6": {
          name: "Kimi K2.6",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.5": {
          name: "Kimi K2.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m2.7": {
          name: "MiniMax-M2.7",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 204800,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "mimo-v2.5-pro": {
          name: "MiMo-V2.5-Pro",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tencent TokenHub (Intl)",
    family: "tencent",
    planKey: "payg",
    regionKey: "intl",
    providerKey: "cc-switch-tencent-tokenhub-intl",
    websiteUrl: "https://www.tencentcloud.com/products/tokenhub",
    apiKeyUrl: "https://console.tencentcloud.com/tokenhub/apikey",
    settingsConfig: {
      name: "Tencent TokenHub (Intl)",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://tokenhub-intl.tencentcloudmaas.com/v1",
        apiKey: "",
      },
      models: {
        "hy4-preview": {
          name: "Hy4 Preview",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 65536,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        hy3: {
          name: "Hy3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "deepseek-v4-flash-202605": {
          name: "DeepSeek V4 Flash Official",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek/deepseek-v4-flash-vision-exp": {
          name: "DeepSeek V4 Flash Vision Exp",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash-0731": {
          name: "DeepSeek V4 Flash 0731 GA",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v4-flash": {
          name: "DeepSeek V4 Flash",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "deepseek-v3.2": {
          name: "DeepSeek V3.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 131072,
            output: 32768,
          },
          compat: {
            supportsStore: false,
            supportsDeveloperRole: false,
            maxTokensField: "max_tokens",
            thinkingFormat: "deepseek",
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            high: "high",
            max: "max",
          },
        },
        "glm-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.3-flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.2": {
          name: "GLM-5.2",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5": {
          name: "GLM-5",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5-turbo": {
          name: "GLM-5 Turbo",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5v-turbo": {
          name: "GLM-5V Turbo",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "glm-5.1": {
          name: "GLM-5.1",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 200000,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3": {
          name: "Kimi K3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code-highspeed": {
          name: "Kimi K2.7 Code HighSpeed",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.7-code": {
          name: "Kimi K2.7 Code",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.6": {
          name: "Kimi K2.6",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k2.5": {
          name: "Kimi K2.5",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 262144,
            output: 262144,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "minimax-m2.7": {
          name: "MiniMax-M2.7",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 204800,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "mimo-v2.5-pro": {
          name: "MiMo-V2.5-Pro",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "cn_official",
    icon: "tencent",
    iconColor: "#00A4FF",
  },
  {
    name: "Tu-zi",
    nameKey: "providerForm.presets.tuzi",
    providerKey: "cc-switch-tuzi",
    websiteUrl: "https://api.tu-zi.com",
    apiKeyUrl: "https://api.tu-zi.com/token",
    settingsConfig: {
      name: "Tu-zi",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.tu-zi.com",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "tuzi",
  },
  {
    name: "模力方舟",
    providerKey: "cc-switch-moark",
    websiteUrl: "https://moark.com",
    apiKeyUrl: "https://moark.com/dashboard/tokens",
    settingsConfig: {
      name: "模力方舟",
      kind: "custom",
      enabled: true,
      api: "openai-completions",
      options: {
        baseURL: "https://api.moark.com/v1",
        apiKey: "",
      },
      models: {
        "deepseek-v4.1-flash": {
          name: "DeepSeek V4.1 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 384000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "GLM-5.3": {
          name: "GLM-5.3",
          modalities: {
            input: ["text"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "GLM-5.3-Flash": {
          name: "GLM-5.3 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "kimi-k3.1-pro": {
          name: "Kimi K3.1 Pro",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1048576,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3.8-max": {
          name: "Qwen3.8 Max",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "qwen3.8-flash": {
          name: "Qwen3.8 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 983616,
            output: 131072,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "MiniMax-M3": {
          name: "MiniMax-M3",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 1000000,
            output: 128000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
        "Step-3.7-Flash": {
          name: "Step 3.7 Flash",
          modalities: {
            input: ["text", "image"],
            output: ["text"],
          },
          limit: {
            context: 256000,
            output: 256000,
          },
          reasoning: true,
          reasoningEfforts: {
            off: null,
            minimal: "minimal",
            low: "low",
            medium: "medium",
            high: "high",
          },
        },
      },
    },
    category: "aggregator",
    icon: "moark",
  },
  {
    name: "SoruxGPT",
    providerKey: "cc-switch-sorux-gpt",
    websiteUrl: "https://ai.soruxgpt.com",
    apiKeyUrl: "https://ai.soruxgpt.com/console/token",
    settingsConfig: {
      name: "SoruxGPT",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://ai.soruxgpt.com",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "soruxgpt",
  },
  {
    name: "IKunCode",
    providerKey: "cc-switch-ikun-code",
    websiteUrl: "https://api.ikuncode.ai",
    apiKeyUrl: "https://api.ikuncode.ai/console/token",
    settingsConfig: {
      name: "IKunCode",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.ikuncode.ai",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "ikuncode",
  },
  {
    name: "RunToken",
    providerKey: "cc-switch-run-token",
    websiteUrl: "https://www.runtoken.ai",
    apiKeyUrl: "https://www.runtoken.ai/console/token",
    settingsConfig: {
      name: "RunToken",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://www.runtoken.ai",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "runtoken",
  },
  {
    name: "BotCF",
    providerKey: "cc-switch-bot-cf",
    websiteUrl: "https://botcf.com",
    apiKeyUrl: "https://botcf.com/console/token",
    settingsConfig: {
      name: "BotCF",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://botcf.com",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "botcf",
  },
  {
    name: "YYLX",
    nameKey: "providerForm.presets.yylx",
    providerKey: "cc-switch-yylx",
    websiteUrl: "https://yylx.io",
    apiKeyUrl: "https://app.yylx.io/keys",
    settingsConfig: {
      name: "YYLX",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://app.yylx.io",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "yylx",
  },
  {
    name: "TokenFlux",
    providerKey: "cc-switch-token-flux",
    websiteUrl: "https://tokenflux.com",
    apiKeyUrl: "https://tokenflux.com/keys",
    settingsConfig: {
      name: "TokenFlux",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://tokenflux.com",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "tokenflux",
  },
  {
    name: "NekoCode",
    providerKey: "cc-switch-neko-code",
    websiteUrl: "https://nekoapi.ai",
    apiKeyUrl: "https://nekoapi.ai/token",
    settingsConfig: {
      name: "NekoCode",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://nekoapi.ai",
        apiKey: "",
        headers: {
          "User-Agent": "claude-cli/2.1.161 (external, cli)",
        },
      },
      models: claudeOpus55Models4(),
    },
    category: "aggregator",
    icon: "nekocode",
  },
  {
    name: "AIComing",
    providerKey: "cc-switch-aicoming",
    websiteUrl: "https://aicoming.top",
    apiKeyUrl: "https://aicoming.top/console#keys",
    settingsConfig: {
      name: "AIComing",
      kind: "custom",
      enabled: true,
      api: "anthropic-messages",
      options: {
        baseURL: "https://api.aicoming.top",
        apiKey: "",
      },
      models: claudeOpus55Models3(),
    },
    category: "aggregator",
    icon: "aicoming",
  },
];
export const DSH_ACCOUNT_PRESET_ID = "dsh-official-account";
export const dshAccountPreset: DshProviderPreset = {
  name: "DeepSeek",
  nameKey: "dsh.account.presetName",
  providerKey: "dsh-official-account",
  websiteUrl: "https://platform.deepseek.com",
  icon: "deepseek",
  category: "official",
  settingsConfig: {
    name: "DeepSeek",
    kind: "custom",
    enabled: true,
    api: "openai-completions",
    options: {
      baseURL: "",
      apiKey: "",
    },
    models: {},
  },
};
