import { describe, expect, it } from "vitest";
import {
  detectBalanceProvider,
  matchingBalanceProviders,
} from "@/config/balanceProviders";

describe("balance provider URL boundaries", () => {
  it.each([
    "https://api.deepseek.com.evil.example/v1",
    "https://proxy.example/api.stepfun.ai/v1",
    "https://evil.example/?upstream=openrouter.ai",
    "https://api.siliconflow.cn@evil.example/v1",
    "ftp://api.novita.ai/v3/openai",
    "not a URL",
    "",
    undefined,
  ])("rejects unrelated URLs: %s", (baseUrl) => {
    expect(detectBalanceProvider(baseUrl)).toBe(false);
    expect(matchingBalanceProviders(baseUrl)).toEqual([]);
  });

  it.each([
    ["https://API.DEEPSEEK.COM.:8443/v1", "deepseek"],
    ["openrouter.ai/api/v1", "openrouter"],
    ["https://api.stepfun.ai/v1", "stepfun"],
    ["https://api.stepfun.com/v1", "stepfun"],
    ["https://api.siliconflow.cn/v1", "siliconflow"],
    ["https://api.siliconflow.com/v1", "siliconflow"],
    ["https://api.novita.ai/v3/openai", "novita"],
  ])("matches the parsed host: %s", (baseUrl, expected) => {
    expect(detectBalanceProvider(baseUrl)).toBe(true);
    expect(
      matchingBalanceProviders(baseUrl).map((provider) => provider.id),
    ).toEqual([expected]);
  });
});
