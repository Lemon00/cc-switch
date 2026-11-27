import type { AppId } from "@/lib/api";
import {
  streamCheckProvider,
  type StreamCheckResult,
} from "@/lib/api/connectivity-check";
import { toast } from "@/lib/toast";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
/**
 * 供应商连通性检查。
 *
 * 只探测 base_url 是否可达（任何 HTTP 响应都算可达），不发真实大模型请求。
 * 刻意 **不** 重置故障转移熔断器——可达 ≠ 配置正确，一个端口通但鉴权废的供应商
 * 不应被误判为"健康"而切回线上。熔断器只由真实转发流量驱动（见 proxy/forwarder.rs）。
 */
export function useStreamCheck(appId: AppId) {
  const { t } = useTranslation();
  const [checkingIds, setCheckingIds] = useState<Set<string>>(new Set());
  const pending = useRef(new Map<string, Promise<StreamCheckResult | null>>());
  const generation = useRef(0);
  const currentApp = useRef(appId);
  currentApp.current = appId;
  useEffect(() => {
    const requests = pending.current;
    setCheckingIds(new Set());
    return () => {
      generation.current += 1;
      requests.clear();
    };
  }, [appId]);
  const checkProvider = useCallback(
    (
      providerId: string,
      providerName: string,
    ): Promise<StreamCheckResult | null> => {
      const key = `${appId}:${providerId}`;
      const existing = pending.current.get(key);
      if (existing) return existing;
      const started = generation.current;
      const isCurrent = () =>
        generation.current === started && currentApp.current === appId;
      setCheckingIds((prev) => new Set(prev).add(key));
      const request = Promise.resolve().then(async () => {
        try {
          const result = await streamCheckProvider(appId, providerId);
          if (!isCurrent()) return result;
          if (result.status === "operational") {
            toast.success(
              t("streamCheck.reachable", {
                providerName: providerName,
                responseTimeMs: result.responseTimeMs,
                defaultValue: `${providerName} 连通正常 (${result.responseTimeMs}ms)`,
              }),
              { closeButton: true },
            );
          } else if (result.status === "degraded") {
            toast.warning(
              t("streamCheck.reachableSlow", {
                providerName: providerName,
                responseTimeMs: result.responseTimeMs,
                defaultValue: `${providerName} 连通但较慢 (${result.responseTimeMs}ms)`,
              }),
            );
          } else {
            // 仅当无法建立连接（DNS / 连接被拒 / TLS / 超时）才会到这里
            toast.error(
              t("streamCheck.unreachable", {
                providerName: providerName,
                message: result.message,
                defaultValue: `${providerName} 无法连通: ${result.message}`,
              }),
              {
                description: t("streamCheck.unreachableHint", {
                  defaultValue:
                    "无法建立连接（DNS / 连接 / TLS / 超时）。请检查 base_url 与网络。",
                }),
                duration: 8000,
                closeButton: true,
              },
            );
          }
          return result;
        } catch (e) {
          if (!isCurrent()) return null;
          toast.error(
            t("streamCheck.error", {
              providerName: providerName,
              error: String(e),
              defaultValue: `${providerName} 检查出错: ${String(e)}`,
            }),
          );
          return null;
        } finally {
          if (pending.current.get(key) === request) pending.current.delete(key);
          if (isCurrent()) {
            setCheckingIds((prev) => {
              const next = new Set(prev);
              next.delete(key);
              return next;
            });
          }
        }
      });
      pending.current.set(key, request);
      return request;
    },
    [appId, t],
  );
  const isChecking = useCallback(
    (providerId: string) => checkingIds.has(`${appId}:${providerId}`),
    [appId, checkingIds],
  );
  return { checkProvider, isChecking };
}
