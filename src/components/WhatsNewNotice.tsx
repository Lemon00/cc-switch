import { useNewLayoutNoticePending } from "@/components/shell/NewLayoutDialog";
import { settingsApi } from "@/lib/api";
import { useSettingsQuery } from "@/lib/query";
import {
  entriesSince,
  isNewerThanSeen,
  markSeen,
  WHATS_NEW_ENTRIES,
  type WhatsNewEntry,
} from "@/lib/whatsNew";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getVersion } from "@tauri-apps/api/app";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
const WhatsNewDialog = lazy(() =>
  import("./WhatsNewDialog").then((m) => ({ default: m.WhatsNewDialog })),
);
export function WhatsNewNotice() {
  const queryClient = useQueryClient();
  const { data: settings } = useSettingsQuery();
  const newLayoutPending = useNewLayoutNoticePending(settings);
  const { data: currentVersion } = useQuery({
    queryKey: ["app-version"],
    queryFn: () => getVersion(),
    staleTime: Infinity,
  });
  const [closed, setClosed] = useState(false);
  const stampedRef = useRef(false);
  const seen = settings?.independentWhatsNewSeenVersion;
  const due =
    settings != null &&
    settings.firstRunNoticeConfirmed === true &&
    newLayoutPending === false &&
    !!currentVersion &&
    isNewerThanSeen(currentVersion, seen);
  const entries = useMemo(
    () =>
      due && currentVersion
        ? entriesSince(WHATS_NEW_ENTRIES, seen, currentVersion)
        : [],
    [due, seen, currentVersion],
  );
  // 打开时把内容定格：确认后设置刷新、区间变空，关闭动画里内容不会被清掉
  const [shown, setShown] = useState<{
    entries: WhatsNewEntry[];
    since?: string;
  } | null>(null);
  if (shown === null && entries.length > 0) {
    setShown({ entries, since: seen });
  }
  const acknowledge = useCallback(async () => {
    setClosed(true);
    if (!settings || !currentVersion) return;
    try {
      await settingsApi.save({
        independentWhatsNewSeenVersion: markSeen(
          settings.independentWhatsNewSeenVersion,
          currentVersion,
        ),
      });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    } catch (error) {
      console.error("Failed to save independentWhatsNewSeenVersion:", error);
    }
  }, [settings, currentVersion, queryClient]);
  // 区间里没有可显示的内容：静默记下当前版本，下次升级才有准确的起点
  useEffect(() => {
    if (!due || entries.length > 0 || stampedRef.current) return;
    stampedRef.current = true;
    void acknowledge();
  }, [due, entries.length, acknowledge]);
  if (!shown) return null;
  return (
    <Suspense fallback={null}>
      <WhatsNewDialog
        open={!closed}
        onClose={() => void acknowledge()}
        entries={shown.entries}
        updatedTo={currentVersion}
        since={shown.since}
      />
    </Suspense>
  );
}
