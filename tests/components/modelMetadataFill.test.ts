import { describe, expect, it } from "vitest";
import {
  fillCodexCatalogModel,
  fillOpenCodeModel,
} from "@/components/providers/forms/modelMetadataFill";
import type { KnownModelMetadata } from "@/lib/modelMetadata";
const metadata: KnownModelMetadata = {
  contextWindow: 262144,
  maxOutputTokens: 32768,
  reasoning: true,
  reasoningEfforts: ["max", "low", "turbo", "high"],
  inputModalities: ["text", "image", "video"],
  outputModalities: ["text"],
  cost: { input: 1, output: 4 },
};
const CODEX_LEVELS = ["none", "low", "medium", "high", "xhigh", "max"];
describe("fillCodexCatalogModel", () => {
  it("fills blank fields in Codex's level order", () => {
    expect(
      fillCodexCatalogModel(
        { model: "kimi-k2.6", contextWindow: "" },
        metadata,
        CODEX_LEVELS,
      ),
    ).toEqual({
      model: "kimi-k2.6",
      contextWindow: "262144",
      reasoningLevels: ["low", "high", "max"],
      inputModalities: ["text", "image"],
    });
  });
  it("never overwrites what the user already set", () => {
    const row = {
      model: "kimi-k2.6",
      contextWindow: "128000",
      reasoningLevels: ["high"],
      inputModalities: ["text"],
    };
    expect(fillCodexCatalogModel(row, metadata, CODEX_LEVELS)).toEqual(row);
  });
  it("never turns image input off for a text-only lookup", () => {
    expect(
      fillCodexCatalogModel(
        { model: "deepseek-v4.1-flash-1017", contextWindow: "" },
        { contextWindow: 1000000, inputModalities: ["text"] },
        CODEX_LEVELS,
      ),
    ).toEqual({
      model: "deepseek-v4.1-flash-1017",
      contextWindow: "1000000",
    });
  });
});

describe("fillOpenCodeModel", () => {
  it("fills missing limits, modalities and reasoning", () => {
    expect(fillOpenCodeModel({ name: "m" }, metadata)).toEqual({
      name: "m",
      limit: { context: 262144, output: 32768 },
      reasoning: true,
      modalities: { input: ["text", "image", "video"], output: ["text"] },
    });
  });
  it("only fills reasoning towards true and keeps an explicit value", () => {
    expect(fillOpenCodeModel({ name: "m" }, { reasoning: false })).toEqual({
      name: "m",
    });
    expect(
      fillOpenCodeModel({ name: "m", reasoning: false }, { reasoning: true }),
    ).toEqual({ name: "m", reasoning: false });
  });
  it("only fills the missing half of limit", () => {
    expect(
      fillOpenCodeModel(
        {
          name: "m",
          limit: { context: 1000 },
          modalities: { input: ["text"] },
        },
        metadata,
      ),
    ).toEqual({
      name: "m",
      limit: { context: 1000, output: 32768 },
      modalities: { input: ["text"] },
      reasoning: true,
    });
  });
  it("leaves the model untouched when nothing is known", () => {
    expect(fillOpenCodeModel({ name: "m" }, {})).toEqual({ name: "m" });
  });
});
