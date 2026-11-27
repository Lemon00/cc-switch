import { useSyncExternalStore } from "react";

let available = false;
const listeners = new Set<() => void>();
export function publishToolUpdates(value: boolean) {
  if (available === value) return;
  available = value;
  listeners.forEach((listener) => listener());
}
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export function useToolUpdatesAvailable() {
  return useSyncExternalStore(
    subscribe,
    () => available,
    () => false,
  );
}
export async function checkToolUpdatesInBackground() {
  const tools = await import("@/components/apps/useToolManagement");
  await tools.checkToolUpdatesInBackground();
}
