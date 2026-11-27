import type { EditorConflictPolicy } from "@/lib/api/providers";
import { parseLiveEditConflict } from "@/lib/errors/liveEditConflict";
import { lazy, Suspense, useCallback, useState } from "react";
const LiveEditConflictDialog = lazy(() =>
  import("@/components/providers/LiveEditConflictDialog").then((m) => ({
    default: m.LiveEditConflictDialog,
  })),
);
/**
 * 编辑器保存的冲突重试：先按 `refuse` 提交；配置文件在编辑期间被改过时弹出
 * {@link LiveEditConflictDialog}，按用户选的策略再提交一次。重试失败的提示由保存的
 * mutation 负责，编辑器保持打开。
 */
export function useLiveEditConflict() {
  const [pendingConflict, setPendingConflict] = useState<{
    keys: string[];
    retry: (policy: EditorConflictPolicy) => Promise<void>;
  } | null>(null);
  const [isResolving, setIsResolving] = useState(false);
  const submitWithConflictRetry = useCallback(
    async (submit: (policy: EditorConflictPolicy) => Promise<void>) => {
      try {
        await submit("refuse");
      } catch (error) {
        const conflict = parseLiveEditConflict(error);
        if (!conflict) throw error;
        setPendingConflict({ keys: conflict.keys, retry: submit });
      }
    },
    [],
  );
  const resolve = useCallback(
    async (policy: EditorConflictPolicy) => {
      if (!pendingConflict) return;
      setIsResolving(true);
      try {
        await pendingConflict.retry(policy);
      } catch {
        // 失败提示由保存的 mutation 负责。
      } finally {
        setIsResolving(false);
        setPendingConflict(null);
      }
    },
    [pendingConflict],
  );
  const conflictDialog = pendingConflict ? (
    <Suspense fallback={false}>
      <LiveEditConflictDialog
        keys={pendingConflict?.keys ?? null}
        pending={isResolving}
        onResolve={(policy) => void resolve(policy)}
        onCancel={() => setPendingConflict(null)}
      />
    </Suspense>
  ) : null;
  return { submitWithConflictRetry, conflictDialog };
}
