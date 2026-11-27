import { useCallback, useEffect, useRef, useState } from "react";
import { trayPanelApi, type TrayPanelState } from "./api";
/** One read at a time; a change during a read requires a fresh snapshot. */
export function usePanelState() {
  const [state, setState] = useState<TrayPanelState | null>(null);
  const lastState = useRef("");
  const generation = useRef(0);
  const flight = useRef<Promise<void> | null>(null);
  const requested = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
      flight.current = null;
    },
    [],
  );
  const load = useCallback(() => {
    requested.current++;
    if (flight.current) return flight.current;
    const currentGeneration = generation.current;
    const run = async () => {
      let revision: number;
      do {
        revision = requested.current;
        try {
          const next = await trayPanelApi.state();
          if (currentGeneration !== generation.current) return;
          // A notification arrived while reading. Do not flash the older snapshot.
          if (revision !== requested.current) continue;
          const key = JSON.stringify(next);
          if (key !== lastState.current) {
            lastState.current = key;
            setState(next);
          }
        } catch (error) {
          if (currentGeneration !== generation.current) return;
          console.error("[TrayPanel] 读取托盘面板数据失败", error);
        }
      } while (revision !== requested.current);
    };
    flight.current = run().finally(() => {
      if (currentGeneration === generation.current) flight.current = null;
    });
    return flight.current;
  }, []);
  const clearFeedback = useCallback(() => {
    if (!lastState.current) return;
    const previous = JSON.parse(lastState.current) as TrayPanelState;
    if (!previous.feedback) return;
    const next = { ...previous, feedback: null };
    lastState.current = JSON.stringify(next);
    setState(next);
  }, []);
  return { state, load, clearFeedback, hasSnapshot: lastState };
}
