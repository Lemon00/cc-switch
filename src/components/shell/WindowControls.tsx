import { HoverTip } from "@/components/ui/hover-tip";
import { cleanupTauriListener } from "@/lib/tauriEventCleanup";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { extractErrorMessage } from "@/utils/errorUtils";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
const BUTTON =
  "flex h-7 w-7 items-center justify-center rounded-control text-fg-2 transition-colors hover:bg-subtle hover:text-fg-1";
/** 接在页头最右端的最小化 / 最大化 / 关闭：Windows 一律用它，Linux 打开「使用应用内窗口按钮」后用它。 */
export function WindowControls() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);
  const refreshMaximizedRef = useRef(() => {});
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    let reading = false;
    let refreshAgain = false;
    // Resize can fire repeatedly while dragging. Keep one IPC read in flight
    // and read again after it settles if the native state changed meanwhile.
    const refresh = () => {
      if (!active) return;
      refreshAgain = true;
      if (reading) return;
      reading = true;
      void (async () => {
        try {
          while (active && refreshAgain) {
            refreshAgain = false;
            try {
              const value = await getCurrentWindow().isMaximized();
              if (active && !refreshAgain) setMaximized(value);
            } catch (error) {
              if (active) {
                console.error(
                  "[WindowControls] Failed to sync maximized state",
                  error,
                );
              }
            }
          }
        } finally {
          reading = false;
        }
      })();
    };
    refreshMaximizedRef.current = refresh;
    const sync = async () => {
      try {
        // Subscribe before reading so changes during registration are included.
        const off = await getCurrentWindow().onResized(refresh);
        if (!active) {
          cleanupTauriListener(off, "tauri://resize");
          return;
        }
        unlisten = off;
      } catch (error) {
        console.error("[WindowControls] Failed to sync maximized state", error);
      }
      refresh();
    };
    void sync();
    return () => {
      active = false;
      refreshMaximizedRef.current = () => {};
      if (unlisten) cleanupTauriListener(unlisten, "tauri://resize");
    };
  }, []);
  const run = async (action: () => Promise<void>) => {
    try {
      await action();
    } catch (error) {
      console.error("[WindowControls] Window control failed", error);
      toast.error(
        t("notifications.windowControlFailed", {
          defaultValue: "窗口控制失败：{{error}}",
          error: extractErrorMessage(error),
        }),
      );
    }
  };
  const maximizeLabel = maximized
    ? t("header.windowRestore")
    : t("header.windowMaximize");
  return (
    <>
      <HoverTip content={t("header.windowMinimize")}>
        <button
          type="button"
          className={BUTTON}
          aria-label={t("header.windowMinimize")}
          onClick={() => void run(() => getCurrentWindow().minimize())}
        >
          <Minus className="h-4 w-4" />
        </button>
      </HoverTip>
      <HoverTip content={maximizeLabel}>
        <button
          type="button"
          className={BUTTON}
          aria-label={maximizeLabel}
          onClick={() =>
            void run(async () => {
              const win = getCurrentWindow();
              await win.toggleMaximize();
              refreshMaximizedRef.current();
            })
          }
        >
          {maximized ? (
            <Minimize2 className="h-4 w-4" />
          ) : (
            <Maximize2 className="h-4 w-4" />
          )}
        </button>
      </HoverTip>
      <HoverTip content={t("header.windowClose")}>
        <button
          type="button"
          className={cn(BUTTON, "hover:bg-danger-soft hover:text-danger-text")}
          aria-label={t("header.windowClose")}
          onClick={() => void run(() => getCurrentWindow().close())}
        >
          <X className="h-4 w-4" />
        </button>
      </HoverTip>
    </>
  );
}
