import { cleanupTauriListener } from "@/lib/tauriEventCleanup";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { lazy, Suspense, useEffect, useState } from "react";

const Dialog = lazy(() =>
  import("./DeepLinkImportDialog").then((m) => ({
    default: m.DeepLinkImportDialog,
  })),
);

export function DeepLinkImportHost() {
  const [active, setActive] = useState(false);
  useEffect(() => {
    let disposed = false;
    const activate = () => {
      if (!disposed) setActive(true);
    };
    const listener = listen("deeplink-pending", activate);
    const checkPending = () =>
      invoke<unknown>("peek_pending_deeplink")
        .then((pending) => {
          if (pending) activate();
        })
        .catch((error) =>
          console.error("Failed to check pending deep link", error),
        );
    void listener.then(checkPending).catch(checkPending);
    return () => {
      disposed = true;
      void listener
        .then((off) => cleanupTauriListener(off, "deeplink-pending"))
        .catch(() => {});
    };
  }, []);
  return active ? (
    <Suspense fallback={null}>
      <Dialog />
    </Suspense>
  ) : null;
}
