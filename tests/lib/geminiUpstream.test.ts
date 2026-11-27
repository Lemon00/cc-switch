import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fetchModelsForConfig,
  upstreamModelFetchApiFormat,
} from "@/lib/api/model-fetch";
import { opencodeNpmPackages } from "@/config/opencodeProviderPresets";
const invoke = vi.hoisted(() => vi.fn().mockResolvedValue([]));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
describe("Gemini 上游格式", () => {
  beforeEach(() => {
    invoke.mockClear();
  });
  it.each(["claude", "codex"])("%s 使用 Google 模型目录协议", async () => {
    await fetchModelsForConfig(
      "https://upstream.example/v1beta",
      "test-key",
      false,
      undefined,
      undefined,
      {
        apiFormat: upstreamModelFetchApiFormat("gemini_native"),
      },
    );
    expect(invoke).toHaveBeenCalledWith(
      "fetch_models_for_config",
      expect.objectContaining({ apiFormat: "google-generative-ai" }),
    );
  });
  it("OpenCode 提供原生 Google SDK 选项", () => {
    expect(opencodeNpmPackages).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ value: "@ai-sdk/google" }),
      ]),
    );
    expect(upstreamModelFetchApiFormat("@ai-sdk/google")).toBe(
      "google-generative-ai",
    );
  });
  it.each(["anthropic", "openai_chat", "openai_responses", undefined])(
    "保留 %s 的默认目录协议",
    (format) => {
      expect(upstreamModelFetchApiFormat(format)).toBeUndefined();
    },
  );
});
