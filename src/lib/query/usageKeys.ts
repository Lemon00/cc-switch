import type { UsageRangeSelection, UsageScopeFilters } from "@/types/usage";
import type { QueryFilters } from "@tanstack/react-query";
export type RequestLogsKey = {
  preset: UsageRangeSelection["preset"];
  customStartDate?: number;
  customEndDate?: number;
  liveEndTime?: boolean;
  appType?: string;
  providerName?: string;
  model?: string;
  statusCode?: number;
};
// Query keys
export const usageKeys = {
  all: ["usage"] as const,
  summary: (
    preset: UsageRangeSelection["preset"],
    customStartDate: number | undefined,
    customEndDate: number | undefined,
    filters?: UsageScopeFilters,
    liveEndTime?: boolean,
  ) =>
    [
      ...usageKeys.all,
      "summary",
      preset,
      customStartDate ?? 0,
      customEndDate ?? 0,
      liveEndTime ?? false,
      filters?.appType ?? null,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  summaryByApp: (
    preset: UsageRangeSelection["preset"],
    customStartDate: number | undefined,
    customEndDate: number | undefined,
    filters?: Pick<UsageScopeFilters, "providerName" | "model">,
    liveEndTime?: boolean,
  ) =>
    [
      ...usageKeys.all,
      "summary-by-app",
      preset,
      customStartDate ?? 0,
      customEndDate ?? 0,
      liveEndTime ?? false,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  trends: (
    preset: UsageRangeSelection["preset"],
    customStartDate: number | undefined,
    customEndDate: number | undefined,
    filters?: UsageScopeFilters,
    liveEndTime?: boolean,
  ) =>
    [
      ...usageKeys.all,
      "trends",
      preset,
      customStartDate ?? 0,
      customEndDate ?? 0,
      liveEndTime ?? false,
      filters?.appType ?? null,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  firstDate: (filters?: UsageScopeFilters) =>
    [
      ...usageKeys.all,
      "first-date",
      filters?.appType ?? null,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  providerStats: (
    preset: UsageRangeSelection["preset"],
    customStartDate: number | undefined,
    customEndDate: number | undefined,
    filters?: UsageScopeFilters,
    liveEndTime?: boolean,
  ) =>
    [
      ...usageKeys.all,
      "provider-stats",
      preset,
      customStartDate ?? 0,
      customEndDate ?? 0,
      liveEndTime ?? false,
      filters?.appType ?? null,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  modelStats: (
    preset: UsageRangeSelection["preset"],
    customStartDate: number | undefined,
    customEndDate: number | undefined,
    filters?: UsageScopeFilters,
    liveEndTime?: boolean,
  ) =>
    [
      ...usageKeys.all,
      "model-stats",
      preset,
      customStartDate ?? 0,
      customEndDate ?? 0,
      liveEndTime ?? false,
      filters?.appType ?? null,
      filters?.providerName ?? null,
      filters?.model ?? null,
    ] as const,
  logs: (key: RequestLogsKey, page: number, pageSize: number) =>
    [
      ...usageKeys.all,
      "logs",
      key.preset,
      key.customStartDate ?? 0,
      key.customEndDate ?? 0,
      key.liveEndTime ?? false,
      key.appType ?? "",
      key.providerName ?? "",
      key.model ?? "",
      key.statusCode ?? -1,
      page,
      pageSize,
    ] as const,
  detail: (requestId: string) =>
    [...usageKeys.all, "detail", requestId] as const,
  pricing: () => [...usageKeys.all, "pricing"] as const,
  session: (appType: string, sessionId: string) =>
    [...usageKeys.all, "session", appType, sessionId] as const,
  limits: (providerId: string, appType: string) =>
    [...usageKeys.all, "limits", providerId, appType] as const,
  script: (providerId: string, appType: string) =>
    [...usageKeys.all, "script", providerId, appType] as const,
};
/** 远程脚本用量 key：本地定价变更不应触发重取 */
export function isRemoteScriptUsageQueryKey(
  queryKey: readonly unknown[],
): boolean {
  return queryKey[0] === usageKeys.all[0] && queryKey[1] === "script";
}
/** 本地日志、价格与统计变更不影响远端额度；所有本地刷新入口共用。 */
export const localUsageQueryFilters = {
  queryKey: usageKeys.all,
  predicate: (query) => !isRemoteScriptUsageQueryKey(query.queryKey),
} satisfies QueryFilters;
