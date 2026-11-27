import { describe, expect, it } from "vitest";
import { flattenModels, toModelPricing } from "@/lib/modelsDevPricing";
import type { ModelsDevResponse } from "@/lib/modelsDev";
describe("remote pricing validation", () => {
  it("skips invalid prices without converting them to free models", () => {
    const models = Object.fromEntries(
      [-1, Infinity, NaN, 1e12, "bad"].map((input, i) => [
        `invalid-${i}`,
        { cost: { input, output: 2 } },
      ]),
    );
    const entries = flattenModels({
      provider: {
        models: {
          ...models,
          valid: { cost: { input: 1, output: 2 } },
          free: { cost: { input: 0, output: 0 } },
          invalidCache: { cost: { input: 1, output: 2, cache_read: -1 } },
        },
      },
    } as ModelsDevResponse);
    expect(
      toModelPricing(entries)
        .map((entry) => entry.modelId)
        .sort(),
    ).toEqual(["free", "valid"]);
  });
  it("handles malformed optional metadata without aborting the whole import", () => {
    const entries = flattenModels({
      provider: {
        name: 1,
        models: {
          model: {
            status: 7,
            name: {},
            modalities: { output: "text" },
            cost: { input: 1, output: 2 },
          },
        },
      },
    } as unknown as ModelsDevResponse);
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      providerName: "provider",
      modelName: "model",
    });
  });
});
