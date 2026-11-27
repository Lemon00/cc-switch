import { parseUsageProviderUrl } from "@/lib/usageProviderUrl";
const BALANCE_PROVIDERS = [
  { id: "deepseek", label: "DeepSeek", pattern: /^api\.deepseek\.com$/i },
  { id: "stepfun", label: "StepFun", pattern: /^api\.stepfun\.(ai|com)$/i },
  {
    id: "siliconflow",
    label: "SiliconFlow",
    pattern: /^api\.siliconflow\.(cn|com)$/i,
  },
  { id: "openrouter", label: "OpenRouter", pattern: /^openrouter\.ai$/i },
  { id: "novita", label: "Novita AI", pattern: /^api\.novita\.ai$/i },
] as const;
export function matchingBalanceProviders(baseUrl: string | null | undefined) {
  const parsed = parseUsageProviderUrl(baseUrl);
  if (!parsed) return [];
  const host = parsed.hostname.replace(/\.$/, "");
  return BALANCE_PROVIDERS.filter((provider) => provider.pattern.test(host));
}
export function detectBalanceProvider(
  baseUrl: string | null | undefined,
): boolean {
  return matchingBalanceProviders(baseUrl).length > 0;
}
