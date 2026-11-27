import { proxyApi } from "@/lib/api/proxy";
import { cleanupTauriListener } from "@/lib/tauriEventCleanup";
import type { StartupAttachFailure } from "@/types/proxy";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
/** 先订阅、再读持久快照，覆盖后台恢复晚于界面挂载和事件早到的情况。 */
export function useStartupFailures() {
  const [failures, setFailures] = useState<StartupAttachFailure[]>([]);
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    const seen = new Set<string>();
    const refresh = async () => {
      if (disposed) return;
      try {
        const snapshot = await proxyApi.takeStartupAttachFailures();
        if (disposed) return;
        const additions = (snapshot ?? []).filter((failure) => {
          const key = JSON.stringify(failure);
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        });
        if (additions.length)
          setFailures((current) => [...current, ...additions]);
      } catch (error) {
        console.error("Failed to read startup attach failures", error);
      }
    };
    void (async () => {
      try {
        const off = await listen(
          "startup-routing-settled",
          () => void refresh(),
        );
        if (disposed) {
          cleanupTauriListener(off, "startup-routing-settled");
          return;
        }
        unlisten = off;
      } catch (error) {
        console.error("Failed to subscribe startup routing status", error);
      }
      if (!disposed) await refresh();
    })();
    return () => {
      disposed = true;
      if (unlisten) cleanupTauriListener(unlisten, "startup-routing-settled");
    };
  }, []);
  return [failures, setFailures] as const;
}
