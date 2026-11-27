import { focusManager } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cleanupTauriListener } from "./tauriEventCleanup";
let initialized = false;
export function initializeWindowActivity() {
  if (initialized) return;
  initialized = true;

  // Visible monitoring windows must keep refreshing while the user works in a
  // terminal. Only hidden/minimized windows pause Query.
  // Native visibility covers WebViews that remain
  // document-visible after being hidden.
  focusManager.setEventListener((handleFocus) => {
    const currentWindow = isTauri() ? getCurrentWindow() : undefined;
    let nativeVisible = true;
    let visibilityRevision = 0;
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    const update = () => {
      const visible = nativeVisible && document.visibilityState !== "hidden";
      handleFocus(visible);
    };
    const refreshVisibility = async () => {
      if (!currentWindow || disposed) return;
      const revision = ++visibilityRevision;
      try {
        const [visible, minimized] = await Promise.all([
          currentWindow.isVisible(),
          currentWindow.isMinimized(),
        ]);
        if (disposed || visibilityRevision !== revision) return;
        nativeVisible = visible && !minimized;
        update();
      } catch (error) {
        console.error("Failed to read window visibility", error);
      }
    };
    const onVisibilityChange = () => {
      update();
      void refreshVisibility();
    };
    const onFocus = onVisibilityChange;
    const onBlur = onVisibilityChange;
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
    update();

    if (currentWindow) {
      const observe = async (register: () => Promise<() => void>) => {
        try {
          const off = await register();
          if (disposed) cleanupTauriListener(off, "window activity");
          else unlisteners.push(off);
        } catch (error) {
          console.error("Failed to observe window activity", error);
        }
      };
      void (async () => {
        await Promise.all([
          observe(() =>
            currentWindow.onFocusChanged(() => {
              if (disposed) return;
              onVisibilityChange();
            }),
          ),
          observe(() =>
            currentWindow.onResized(() => void refreshVisibility()),
          ),
          // Closing an already-unfocused window to the tray may emit no blur
          // or resize. The backend notifies after its explicit show/hide calls.
          observe(() =>
            currentWindow.listen("main-window-visibility-changed", () => {
              void refreshVisibility();
            }),
          ),
        ]);
        if (disposed) return;
        // Register before snapshots so transitions during setup cannot be lost.
        void refreshVisibility();
      })();
    }

    return () => {
      disposed = true;
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisteners.forEach((off) =>
        cleanupTauriListener(off, "window activity"),
      );
    };
  });
}
