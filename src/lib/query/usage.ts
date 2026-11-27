import { usageApi } from "@/lib/api/usage";
import { resolveUsageRange } from "@/lib/usageRange";
import type {
  LogFilters,
  UsageRangeSelection,
  UsageScopeFilters,
} from "@/types/usage";
import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { invalidateQueriesAfterChange } from "./invalidation";
import {
  localUsageQueryFilters,
  usageKeys,
  type RequestLogsKey,
} from "./usageKeys";
const DEFAULT_REFETCH_INTERVAL_MS = 30000;
type UsageQueryOptions = {
  enabled?: boolean;
  refetchInterval?: number | false;
  refetchIntervalInBackground?: boolean;
};
type RequestLogsQueryArgs = {
  filters: LogFilters;
  range: UsageRangeSelection;
  page?: number;
  pageSize?: number;
  options?: UsageQueryOptions;
};
export {
  isRemoteScriptUsageQueryKey,
  localUsageQueryFilters,
  usageKeys,
} from "./usageKeys";
/** 把 UI 侧的 "all" 哨兵归一成 undefined（后端语义：不过滤）。 */
function normalizeScopeFilters(filters?: UsageScopeFilters): UsageScopeFilters {
  return {
    appType: filters?.appType === "all" ? undefined : filters?.appType,
    providerName: filters?.providerName,
    model: filters?.model,
  };
}
// Hooks
// 统计类查询都带 keepPreviousData：换筛选、时间范围、翻页时先留着上一份数据，
// 新数据到了再换，不让指标闪成「…」、请求日志表塌成骨架。
export function useUsageSummary(
  range: UsageRangeSelection,
  filters?: UsageScopeFilters,
  options?: UsageQueryOptions,
) {
  const effective = normalizeScopeFilters(filters);
  return useQuery({
    queryKey: usageKeys.summary(
      range.preset,
      range.customStartDate,
      range.customEndDate,
      effective,
      range.liveEndTime,
    ),
    queryFn: () => {
      const { startDate, endDate } = resolveUsageRange(range);
      return usageApi.getUsageSummary(
        startDate,
        endDate,
        effective.appType,
        effective.providerName,
        effective.model,
      );
    },
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useUsageSummaryByApp(
  range: UsageRangeSelection,
  filters?: Pick<UsageScopeFilters, "providerName" | "model">,
  options?: UsageQueryOptions,
) {
  return useQuery({
    queryKey: usageKeys.summaryByApp(
      range.preset,
      range.customStartDate,
      range.customEndDate,
      filters,
      range.liveEndTime,
    ),
    queryFn: () => {
      const { startDate, endDate } = resolveUsageRange(range);
      return usageApi.getUsageSummaryByApp(
        startDate,
        endDate,
        filters?.providerName,
        filters?.model,
      );
    },
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useUsageTrends(
  range: UsageRangeSelection,
  filters?: UsageScopeFilters,
  options?: UsageQueryOptions,
) {
  const effective = normalizeScopeFilters(filters);
  return useQuery({
    queryKey: usageKeys.trends(
      range.preset,
      range.customStartDate,
      range.customEndDate,
      effective,
      range.liveEndTime,
    ),
    queryFn: () => {
      const { startDate, endDate } = resolveUsageRange(range);
      return usageApi.getUsageTrends(
        startDate,
        endDate,
        effective.appType,
        effective.providerName,
        effective.model,
      );
    },
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
/** 最早有用量记录的日期，「全部」的按年热力图据此决定从哪一年画起 */
export function useUsageFirstDate(
  filters?: UsageScopeFilters,
  options?: UsageQueryOptions,
) {
  const effective = normalizeScopeFilters(filters);
  return useQuery({
    queryKey: usageKeys.firstDate(effective),
    queryFn: () =>
      usageApi.getUsageFirstDate(
        effective.appType,
        effective.providerName,
        effective.model,
      ),
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useProviderStats(
  range: UsageRangeSelection,
  filters?: UsageScopeFilters,
  options?: UsageQueryOptions,
) {
  const effective = normalizeScopeFilters(filters);
  return useQuery({
    queryKey: usageKeys.providerStats(
      range.preset,
      range.customStartDate,
      range.customEndDate,
      effective,
      range.liveEndTime,
    ),
    queryFn: () => {
      const { startDate, endDate } = resolveUsageRange(range);
      return usageApi.getProviderStats(
        startDate,
        endDate,
        effective.appType,
        effective.providerName,
        effective.model,
      );
    },
    placeholderData: keepPreviousData,
    enabled: options?.enabled ?? true,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useModelStats(
  range: UsageRangeSelection,
  filters?: UsageScopeFilters,
  options?: UsageQueryOptions,
) {
  const effective = normalizeScopeFilters(filters);
  return useQuery({
    queryKey: usageKeys.modelStats(
      range.preset,
      range.customStartDate,
      range.customEndDate,
      effective,
      range.liveEndTime,
    ),
    queryFn: () => {
      const { startDate, endDate } = resolveUsageRange(range);
      return usageApi.getModelStats(
        startDate,
        endDate,
        effective.appType,
        effective.providerName,
        effective.model,
      );
    },
    placeholderData: keepPreviousData,
    enabled: options?.enabled ?? true,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useRequestLogs({
  filters,
  range,
  page = 0,
  pageSize = 20,
  options,
}: RequestLogsQueryArgs) {
  const key: RequestLogsKey = {
    preset: range.preset,
    customStartDate: range.customStartDate,
    customEndDate: range.customEndDate,
    liveEndTime: range.liveEndTime,
    appType: filters.appType,
    providerName: filters.providerName,
    model: filters.model,
    statusCode: filters.statusCode,
  };
  return useQuery({
    queryKey: usageKeys.logs(key, page, pageSize),
    queryFn: () => {
      const effectiveFilters = { ...filters, ...resolveUsageRange(range) };
      return usageApi.getRequestLogs(effectiveFilters, page, pageSize);
    },
    placeholderData: keepPreviousData,
    enabled: options?.enabled ?? true,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS, // 每30秒自动刷新
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
export function useRequestDetail(requestId: string) {
  return useQuery({
    queryKey: usageKeys.detail(requestId),
    queryFn: () => usageApi.getRequestDetail(requestId),
    enabled: !!requestId,
  });
}
/**
 * 会话日志扫描（后台定时或手动同步）最近一次完成的时间（毫秒）。
 * 默认 30 秒查询一次；用量页由统一刷新流程在扫描结束后失效重取。
 */
export function useSessionUsageLastSync(options?: UsageQueryOptions) {
  return useQuery({
    queryKey: [...usageKeys.all, "session-last-sync"] as const,
    queryFn: () => usageApi.getSessionUsageLastSync(),
    enabled: options?.enabled ?? true,
    refetchInterval: options?.refetchInterval ?? DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: options?.refetchIntervalInBackground ?? false,
  });
}
/** 单个会话的用量汇总（会话阅读页头部），只数会话日志导入的行。 */
export function useSessionUsageSummary(appType: string, sessionId: string) {
  return useQuery({
    queryKey: usageKeys.session(appType, sessionId),
    queryFn: () => usageApi.getSessionUsageSummary(appType, sessionId),
    refetchInterval: DEFAULT_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: false,
  });
}
export function useModelPricing() {
  return useQuery({
    queryKey: usageKeys.pricing(),
    queryFn: usageApi.getModelPricing,
  });
}
export function useProviderLimits(providerId: string, appType: string) {
  return useQuery({
    queryKey: usageKeys.limits(providerId, appType),
    queryFn: () => usageApi.checkProviderLimits(providerId, appType),
    enabled: !!providerId && !!appType,
  });
}
export function useUpdateModelPricing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      modelId: string;
      displayName: string;
      inputCost: string;
      outputCost: string;
      cacheReadCost: string;
      cacheCreationCost: string;
    }) =>
      usageApi.updateModelPricing(
        params.modelId,
        params.displayName,
        params.inputCost,
        params.outputCost,
        params.cacheReadCost,
        params.cacheCreationCost,
      ),
    onSuccess: () => {
      return invalidateQueriesAfterChange(queryClient, localUsageQueryFilters);
    },
  });
}
export function useDeleteModelPricing() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (modelId: string) => usageApi.deleteModelPricing(modelId),
    onSuccess: () => {
      return invalidateQueriesAfterChange(queryClient, localUsageQueryFilters);
    },
  });
}
