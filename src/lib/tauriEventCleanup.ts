/** Tauri types unlisten as void, but its implementation can return a promise. */
export function cleanupTauriListener(off: () => void, eventName: string): void {
  const report = (error: unknown) => {
    console.error(`Failed to unsubscribe ${eventName} event`, error);
  };
  try {
    void Promise.resolve(off()).catch(report);
  } catch (error) {
    report(error);
  }
}
