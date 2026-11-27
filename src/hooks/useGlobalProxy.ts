/**
 * 全局出站代理 React Hooks
 *
 * 提供获取、设置和测试全局代理的 React Query hooks。
 */
import {
  getGlobalProxyUrl,
  getUpstreamProxyStatus,
  scanLocalProxies,
  setGlobalProxyUrl,
  testProxyUrl,
  type DetectedProxy,
  type ProxyTestResult,
  type UpstreamProxyStatus,
} from "@/lib/api/globalProxy";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import { toast } from "@/lib/toast";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
/**
 * 获取全局代理 URL
 */
export function useGlobalProxyUrl() {
  return useQuery({
    queryKey: ["globalProxyUrl"],
    queryFn: getGlobalProxyUrl,
    staleTime: 30 * 1000, // 30秒内不重新获取，避免展开时闪烁
  });
}
/**
 * 设置全局代理 URL
 */
export function useSetGlobalProxyUrl() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationFn: setGlobalProxyUrl,
    onSuccess: async () => {
      await Promise.all([
        invalidateQueriesAfterChange(queryClient, {
          queryKey: ["globalProxyUrl"],
        }),
        invalidateQueriesAfterChange(queryClient, {
          queryKey: ["upstreamProxyStatus"],
        }),
      ]);
      toast.success(t("settings.globalProxy.saved"));
    },
    onError: (error: unknown) => {
      const message =
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : "Unknown error";
      toast.error(t("settings.globalProxy.saveFailed", { error: message }));
    },
  });
}
/**
 * 测试代理连接
 */
export function useTestProxy() {
  const { t } = useTranslation();
  return useMutation({
    mutationFn: testProxyUrl,
    onSuccess: (result: ProxyTestResult) => {
      if (result.success) {
        toast.success(
          t("settings.globalProxy.testSuccess", { latency: result.latencyMs }),
        );
      } else {
        toast.error(
          t("settings.globalProxy.testFailed", { error: result.error }),
        );
      }
    },
    onError: (error: Error) => {
      toast.error(error.message);
    },
  });
}
/**
 * 获取当前出站代理状态
 */
export function useUpstreamProxyStatus() {
  return useQuery<UpstreamProxyStatus>({
    queryKey: ["upstreamProxyStatus"],
    queryFn: getUpstreamProxyStatus,
  });
}
/**
 * 扫描本地代理
 */
export function useScanProxies() {
  const { t } = useTranslation();
  return useMutation({
    mutationFn: scanLocalProxies,
    onError: (error: Error) => {
      toast.error(
        t("settings.globalProxy.scanFailed", { error: error.message }),
      );
    },
  });
}
export type { DetectedProxy };
