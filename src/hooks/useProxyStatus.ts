import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
/**
 * 代理服务状态管理 Hook
 */
import { proxyApi } from "@/lib/api/proxy";
import {
  proxyKeys,
  useProxyStatusQuery,
  useProxyTakeoverStatus,
} from "@/lib/query/proxy";
import { toast } from "@/lib/toast";
import { refreshProviderTray } from "@/lib/query/providerActions";
import { claudeLoginQueryKeys } from "@/lib/query/providerCache";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
/**
 * 代理服务状态管理
 */
export function useProxyStatus() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  // 查询状态（自动轮询）
  const { data: status } = useProxyStatusQuery();
  // 查询各应用接管状态
  const { data: takeoverStatus } = useProxyTakeoverStatus(false);
  // 启动服务器（总开关：仅启动服务，不接管）
  const startProxyServerMutation = useMutation({
    mutationFn: () => proxyApi.startProxyServer(),
    onSuccess: (info) => {
      toast.success(
        t("proxy.server.started", {
          address: info.address,
          port: info.port,
          defaultValue: `代理服务已启动 - ${info.address}:${info.port}`,
        }),
        { closeButton: true },
      );
      return invalidateQueriesAfterChange(queryClient, {
        queryKey: proxyKeys.status,
      });
    },
    onError: (error: Error) => {
      const detail =
        extractErrorMessage(error) ||
        t("common.unknown", { defaultValue: "未知错误" });
      toast.error(
        t("proxy.server.startFailed", {
          detail,
          defaultValue: `启动代理服务失败: ${detail}`,
        }),
      );
    },
  });
  // 停止服务器（仅停止服务，不改写/恢复其它应用接管状态）
  const stopProxyServerMutation = useMutation({
    mutationFn: () => proxyApi.stopProxyServer(),
    onSuccess: () => {
      toast.success(
        t("proxy.server.stopped", {
          defaultValue: "代理服务已停止",
        }),
        { closeButton: true },
      );
      return invalidateQueriesAfterChange(queryClient, {
        queryKey: proxyKeys.status,
      });
    },
    onError: (error: Error) => {
      const detail =
        extractErrorMessage(error) ||
        t("common.unknown", { defaultValue: "未知错误" });
      toast.error(
        t("proxy.server.stopFailed", {
          detail,
          defaultValue: `停止代理服务失败: ${detail}`,
        }),
      );
    },
  });
  // 停止服务器（总开关关闭：强制恢复所有已接管的 Live 配置）
  const stopWithRestoreMutation = useMutation({
    mutationFn: () => proxyApi.stopProxyWithRestore(),
    onSuccess: () => {
      toast.success(
        t("proxy.stoppedWithRestore", {
          defaultValue: "路由服务已关闭，所有应用已退回直连",
        }),
        { closeButton: true },
      );
    },
    onError: (error: Error) => {
      const detail =
        extractErrorMessage(error) ||
        t("common.unknown", { defaultValue: "未知错误" });
      toast.error(
        t("proxy.stopWithRestoreFailed", {
          detail,
          defaultValue: `停止失败: ${detail}`,
        }),
      );
    },
    onSettled: async () => {
      // Restoring apps is not atomic: even a rejected command can have restored
      // one client. Keep observers subscribed and read back the resulting state.
      await Promise.all(
        [
          proxyKeys.status,
          proxyKeys.takeoverStatus,
          ["providers"],
          ["providerHealth"],
          ...claudeLoginQueryKeys,
        ].map((queryKey) =>
          invalidateQueriesAfterChange(queryClient, { queryKey }),
        ),
      );
      await refreshProviderTray();
    },
  });
  return {
    status,
    isRunning: status?.running,
    takeoverStatus,
    // 启动/停止（总开关）
    startProxyServer: startProxyServerMutation.mutateAsync,
    stopProxyServer: stopProxyServerMutation.mutateAsync,
    stopWithRestore: stopWithRestoreMutation.mutateAsync,
    isPending:
      startProxyServerMutation.isPending ||
      stopProxyServerMutation.isPending ||
      stopWithRestoreMutation.isPending,
  };
}
