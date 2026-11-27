import { getAppLabel } from "@/config/appCapabilities";
import { failoverApi } from "@/lib/api/failover";
import { proxyKeys } from "@/lib/query/proxy";
import { toast } from "@/lib/toast";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { invalidateQueriesAfterChange } from "./invalidation";
// ========== 熔断器 Hooks ==========
/**
 * 获取供应商健康状态
 */
export function useProviderHealth(
  providerId: string,
  appType: string,
  enabled = true,
) {
  return useQuery({
    queryKey: ["providerHealth", providerId, appType],
    queryFn: () => failoverApi.getProviderHealth(providerId, appType),
    enabled: enabled && !!providerId && !!appType,
    refetchInterval: 5000, // 每 5 秒刷新一次
    retry: false,
  });
}
/**
 * 重置熔断器
 *
 * 重置后后端会检查是否应该切回优先级更高的供应商，
 * 因此需要同时刷新供应商列表和代理状态。
 */
export function useResetCircuitBreaker() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      providerId,
      appType,
    }: {
      providerId: string;
      appType: string;
    }) => failoverApi.resetCircuitBreaker(providerId, appType),
    onSuccess: (_, variables) => {
      return Promise.all(
        [
          ["providerHealth", variables.providerId, variables.appType],
          ["circuitBreakerStats", variables.providerId, variables.appType],
          ["providers", variables.appType],
          proxyKeys.status,
        ].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
    },
  });
}
/**
 * 获取熔断器配置
 */
export function useCircuitBreakerConfig() {
  return useQuery({
    queryKey: ["circuitBreakerConfig"],
    queryFn: () => failoverApi.getCircuitBreakerConfig(),
  });
}
/**
 * 更新熔断器配置
 */
export function useUpdateCircuitBreakerConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: failoverApi.updateCircuitBreakerConfig,
    onSuccess: () => {
      // 兼容命令更新所有应用的 proxy_config 行。
      return Promise.all(
        [["circuitBreakerConfig"], ["appProxyConfig"]].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
    },
  });
}
/**
 * 获取熔断器统计信息
 */
export function useCircuitBreakerStats(providerId: string, appType: string) {
  return useQuery({
    queryKey: ["circuitBreakerStats", providerId, appType],
    queryFn: () => failoverApi.getCircuitBreakerStats(providerId, appType),
    enabled: !!providerId && !!appType,
    refetchInterval: 5000, // 每 5 秒刷新一次
  });
}
// ========== 故障转移队列 Hooks（新） ==========
/**
 * 获取故障转移队列
 */
export function useFailoverQueue(appType: string, enabled = true) {
  return useQuery({
    queryKey: ["failoverQueue", appType],
    queryFn: () => failoverApi.getFailoverQueue(appType),
    enabled: enabled && !!appType,
  });
}
/**
 * 获取可添加到队列的供应商
 */
export function useAvailableProvidersForFailover(appType: string) {
  return useQuery({
    queryKey: ["availableProvidersForFailover", appType],
    queryFn: () => failoverApi.getAvailableProvidersForFailover(appType),
    enabled: !!appType,
  });
}
/**
 * 添加供应商到故障转移队列
 */
export function useAddToFailoverQueue() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationFn: ({
      appType,
      providerId,
    }: {
      appType: string;
      providerId: string;
    }) => failoverApi.addToFailoverQueue(appType, providerId),
    onSuccess: (_, variables) => {
      return Promise.all(
        [
          ["failoverQueue", variables.appType],
          ["availableProvidersForFailover", variables.appType],
          ["providers", variables.appType],
        ].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
    },
    onError: (error: Error) => {
      toast.error(
        t("failover.queueAddFailed", {
          detail: extractErrorMessage(error) || t("common.unknown"),
        }),
      );
    },
  });
}
/**
 * 从故障转移队列移除供应商
 */
export function useRemoveFromFailoverQueue() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationFn: ({
      appType,
      providerId,
    }: {
      appType: string;
      providerId: string;
    }) => failoverApi.removeFromFailoverQueue(appType, providerId),
    onSuccess: (_, variables) => {
      return Promise.all(
        [
          ["failoverQueue", variables.appType],
          ["availableProvidersForFailover", variables.appType],
          ["providers", variables.appType],
          ["providerHealth", variables.providerId, variables.appType],
          ["circuitBreakerStats", variables.providerId, variables.appType],
        ].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
    },
    onError: (error: Error) => {
      toast.error(
        t("failover.queueRemoveFailed", {
          detail: extractErrorMessage(error) || t("common.unknown"),
        }),
      );
    },
  });
}
// ========== 自动故障转移开关 Hooks ==========
/**
 * 获取指定应用的自动故障转移开关状态
 */
export function useAutoFailoverEnabled(appType: string, enabled = true) {
  return useQuery({
    queryKey: ["autoFailoverEnabled", appType],
    queryFn: () => failoverApi.getAutoFailoverEnabled(appType),
    enabled: enabled && !!appType,
    // 默认值为 false（与后端保持一致）
    placeholderData: false,
  });
}
/**
 * 设置指定应用的自动故障转移开关状态
 */
export function useSetAutoFailoverEnabled() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationFn: ({ appType, enabled }: { appType: string; enabled: boolean }) =>
      failoverApi.setAutoFailoverEnabled(appType, enabled),
    // 乐观更新
    onMutate: async ({ appType, enabled }) => {
      await queryClient.cancelQueries({
        queryKey: ["autoFailoverEnabled", appType],
      });
      const previousValue = queryClient.getQueryData<boolean>([
        "autoFailoverEnabled",
        appType,
      ]);
      const hadCachedValue = previousValue !== undefined;
      queryClient.setQueryData(["autoFailoverEnabled", appType], enabled);
      return { previousValue, appType, hadCachedValue };
    },
    onSuccess: (_data, variables) => {
      const appLabel = getAppLabel(variables.appType);
      toast.success(
        variables.enabled
          ? t("failover.enabled", {
              app: appLabel,
              defaultValue: `${appLabel} 故障转移已启用`,
            })
          : t("failover.disabled", {
              app: appLabel,
              defaultValue: `${appLabel} 故障转移已关闭`,
            }),
        { closeButton: true },
      );
    },
    // 错误时回滚
    onError: (error: Error, _variables, context) => {
      if (context?.hadCachedValue) {
        queryClient.setQueryData(
          ["autoFailoverEnabled", context.appType],
          context.previousValue,
        );
      } else if (context) {
        queryClient.removeQueries({
          queryKey: ["autoFailoverEnabled", context.appType],
          exact: true,
        });
      }
      const detail =
        extractErrorMessage(error) ||
        t("common.unknown", { defaultValue: "未知错误" });
      toast.error(
        t("failover.toggleFailed", {
          detail,
          defaultValue: `操作失败: ${detail}`,
        }),
      );
    },
    // 无论成功失败，都重新获取
    onSettled: (_, __, variables) => {
      // 主 Tab 开关与 AutoFailoverConfigPanel 共用 appConfig；不刷会导致
      // 面板保存时用旧的 autoFailoverEnabled 覆写刚打开的开关。
      // 启用/关闭故障转移可能触发：
      // - 立即切到队列 P1（当前供应商变化）
      // - 队列为空时自动把当前供应商加入队列（队列内容变化）
      return Promise.all(
        [
          ["autoFailoverEnabled", variables.appType],
          proxyKeys.appConfig(variables.appType),
          ["failoverQueue", variables.appType],
          ["availableProvidersForFailover", variables.appType],
          ["providers", variables.appType],
          proxyKeys.status,
        ].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
    },
  });
}
