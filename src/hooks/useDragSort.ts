import { isProxyAppId } from "@/config/appCapabilities";
import { providersApi, type AppId } from "@/lib/api";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import { toast } from "@/lib/toast";
import type { Provider } from "@/types";
import {
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { arrayMove, sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
export function useDragSort(providers: Record<string, Provider>, appId: AppId) {
  const queryClient = useQueryClient();
  const { t, i18n } = useTranslation();
  const sortedProviders = useMemo(() => {
    const locale =
      i18n.language === "zh"
        ? "zh-CN"
        : i18n.language === "zh-TW"
          ? "zh-TW"
          : "en-US";
    return Object.values(providers).sort((a, b) => {
      if (a.sortIndex !== undefined && b.sortIndex !== undefined) {
        if (a.sortIndex !== b.sortIndex) return a.sortIndex - b.sortIndex;
      } else {
        if (a.sortIndex !== undefined) return -1;
        if (b.sortIndex !== undefined) return 1;
      }
      const timeA = a.createdAt ?? 0;
      const timeB = b.createdAt ?? 0;
      // Missing timestamps must have a fixed position. Falling back to names
      // for only some pairs makes the comparator non-transitive.
      if (timeA !== timeB) {
        return timeA - timeB;
      }
      return a.name.localeCompare(b.name, locale) || a.id.localeCompare(b.id);
    });
  }, [providers, i18n.language]);
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );
  const handleDragEnd = useCallback(
    async (event: DragEndEvent) => {
      const { active, over } = event;
      if (!over || active.id === over.id) {
        return;
      }
      const oldIndex = sortedProviders.findIndex(
        (provider) => provider.id === active.id,
      );
      const newIndex = sortedProviders.findIndex(
        (provider) => provider.id === over.id,
      );
      if (oldIndex === -1 || newIndex === -1) {
        return;
      }
      const reordered = arrayMove(sortedProviders, oldIndex, newIndex);
      const updates = reordered.map((provider, index) => ({
        id: provider.id,
        sortIndex: index,
      }));
      try {
        await providersApi.updateSortOrder(updates, appId);
        await invalidateQueriesAfterChange(queryClient, {
          queryKey: ["providers", appId],
        });
        // Routing apps derive failover order from sort_index.
        if (isProxyAppId(appId)) {
          await invalidateQueriesAfterChange(queryClient, {
            queryKey: ["failoverQueue", appId],
          });
        }
        // 更新托盘菜单以反映新的排序（失败不影响主操作）
        try {
          await providersApi.updateTrayMenu();
        } catch (trayError) {
          console.error("Failed to update tray menu after sort", trayError);
          // 托盘菜单更新失败不影响排序成功
        }
        toast.success(
          t("provider.sortUpdated", {
            defaultValue: "排序已更新",
          }),
          { closeButton: true },
        );
      } catch (error) {
        console.error("Failed to update provider sort order", error);
        // 排序可能已经存下、只是聚合名单没发布成：按库里的顺序重画，再拖一次会重试发布。
        await invalidateQueriesAfterChange(queryClient, {
          queryKey: ["providers", appId],
        });
        toast.error(
          t("provider.sortUpdateFailed", {
            defaultValue: "排序更新失败",
          }),
        );
      }
    },
    [sortedProviders, appId, queryClient, t],
  );
  return {
    sortedProviders,
    sensors,
    handleDragEnd,
  };
}
