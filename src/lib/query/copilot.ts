import { copilotGetUsage, copilotGetUsageForAccount } from "@/lib/api/copilot";
import type { QuotaTier } from "@/types/subscription";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useQuery } from "@tanstack/react-query";
import { useRef } from "react";
import { resolveDisplayUsage, type LastGoodSnapshot } from "./queries";
const REFETCH_INTERVAL = 5 * 60 * 1000; // 5 minutes
export interface CopilotQuota {
  success: boolean;
  plan: string | null;
  resetDate: string | null;
  tiers: QuotaTier[];
  error: string | null;
  queriedAt: number | null;
}
export interface UseCopilotQuotaOptions {
  enabled?: boolean;
  /** 是否启用自动轮询（5 分钟）与窗口 focus 重取 */
  autoQuery?: boolean;
}
const QUERY_REJECTED_PLACEHOLDER: CopilotQuota = {
  success: false,
  plan: null,
  resetDate: null,
  tiers: [],
  error: null,
  queriedAt: null,
};
export function useCopilotQuota(
  accountId: string | null,
  options: UseCopilotQuotaOptions = {},
) {
  const { enabled = true, autoQuery = false } = options;
  const scopeKey = accountId ?? "default";
  const query = useQuery<CopilotQuota>({
    queryKey: ["copilot", "quota", scopeKey],
    queryFn: async (): Promise<CopilotQuota> => {
      const usage = accountId
        ? await copilotGetUsageForAccount(accountId)
        : await copilotGetUsage();
      const premium = usage.quota_snapshots.premium_interactions;
      const utilization =
        premium.entitlement > 0
          ? ((premium.entitlement - premium.remaining) / premium.entitlement) *
            100
          : 0;
      return {
        success: true,
        plan: usage.copilot_plan,
        resetDate: usage.quota_reset_date,
        tiers: [
          {
            name: "premium",
            utilization,
            resetsAt: usage.quota_reset_date,
          },
        ],
        error: null,
        queriedAt: Date.now(),
      };
    },
    enabled,
    refetchInterval: autoQuery ? REFETCH_INTERVAL : false,
    refetchIntervalInBackground: autoQuery,
    refetchOnWindowFocus: autoQuery,
    staleTime: REFETCH_INTERVAL,
    retry: 1,
  });
  // 与订阅额度同一套 keep-last-good：身份切换清空；reject 超窗后透出失败占位
  const lastGoodRef = useRef<{
    key: string;
    snap: LastGoodSnapshot<CopilotQuota> | null;
  }>({ key: scopeKey, snap: null });
  if (lastGoodRef.current.key !== scopeKey) {
    lastGoodRef.current = { key: scopeKey, snap: null };
  }
  const { data, lastGood } = resolveDisplayUsage(
    query.data,
    query.dataUpdatedAt,
    lastGoodRef.current.snap,
    Date.now(),
    { rejected: query.isError },
  );
  lastGoodRef.current.snap = lastGood;
  return {
    ...query,
    data:
      data ??
      (query.isError
        ? {
            ...QUERY_REJECTED_PLACEHOLDER,
            error: extractErrorMessage(query.error) || null,
          }
        : undefined),
  };
}
