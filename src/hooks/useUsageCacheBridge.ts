import type { AppId } from "@/lib/api/types";
import { subscriptionKeys } from "@/lib/query/subscription";
import { usageKeys } from "@/lib/query/usage";
import type { UsageResult } from "@/types";
import type { SubscriptionQuota } from "@/types/subscription";
import { useQueryClient } from "@tanstack/react-query";
import { useTauriEvent } from "./useTauriEvent";
type UsageCacheUpdatedPayload =
  | {
      kind: "script";
      appType: AppId;
      providerId: string;
      data: UsageResult;
    }
  | {
      kind: "subscription";
      appType: AppId;
      data: SubscriptionQuota;
    };
/**
 * 后端 `UsageCache` 写入后会 emit `usage-cache-updated`，本 hook 把 payload 同步到
 * React Query 缓存，让托盘触发的刷新（不经前端）也能立刻反映到主界面，避免
 * React Query 与 Rust 侧两份缓存各自为战。
 */
export function useUsageCacheBridge() {
  const queryClient = useQueryClient();
  useTauriEvent<UsageCacheUpdatedPayload>(
    "usage-cache-updated",
    async (payload) => {
      if (payload.kind === "script") {
        const queryKey = usageKeys.script(payload.providerId, payload.appType);
        // 等待取消引起的状态回滚，再写入事件快照，阻止旧 invoke 迟到覆盖。
        await queryClient.cancelQueries({ queryKey, exact: true });
        queryClient.setQueryData<UsageResult>(queryKey, payload.data);
      } else if (payload.kind === "subscription") {
        const queryKey = subscriptionKeys.quota(payload.appType);
        await queryClient.cancelQueries({ queryKey, exact: true });
        queryClient.setQueryData<SubscriptionQuota>(queryKey, payload.data);
      }
    },
  );
}
