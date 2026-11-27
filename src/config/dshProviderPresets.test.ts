import { describe, expect, it } from "vitest";
import { dshProviderPresets } from "./dshProviderPresets";
describe("DeepSeek Harness presets", () => {
  it("keeps usable native protocols, model limits and input modalities", () => {
    expect(dshProviderPresets.length).toBeGreaterThan(0);
    const keys = new Set<string>();
    for (const preset of dshProviderPresets) {
      expect(keys.has(preset.providerKey)).toBe(false);
      keys.add(preset.providerKey);
      expect(preset.settingsConfig.api).toMatch(
        /^(openai-completions|openai-responses|anthropic-messages)$/,
      );
      expect(preset.settingsConfig.options.baseURL).toMatch(/^https?:\/\//);
      for (const model of Object.values(preset.settingsConfig.models)) {
        expect(model.limit?.context).toBeGreaterThan(0);
        expect(model.limit?.output).toBeGreaterThan(0);
        expect(model.modalities).toEqual(
          expect.objectContaining({
            input: expect.arrayContaining(["text"]),
            output: ["text"],
          }),
        );
      }
    }
  });
  it("keeps reasoning controls and excludes unsupported compatibility flags", () => {
    for (const preset of dshProviderPresets) {
      for (const model of Object.values(preset.settingsConfig.models)) {
        const compat = (model.compat ?? {}) as Record<string, unknown>;
        expect(compat).not.toHaveProperty("deferredToolsMode");
        if (preset.settingsConfig.api !== "openai-completions") {
          expect(compat).not.toHaveProperty("supportsStore");
          expect(compat).not.toHaveProperty("maxTokensField");
        }
        if (model.reasoning) {
          const efforts = model.reasoningEfforts as Record<
            string,
            string | null
          >;
          expect(
            Object.values(efforts).some((value) => typeof value === "string"),
          ).toBe(true);
        }
      }
    }
  });
});
