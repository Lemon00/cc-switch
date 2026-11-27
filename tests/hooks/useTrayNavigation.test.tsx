import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import {
  parseTrayNavigation,
  useTrayAppPageSeen,
  useTrayNavigation,
} from "@/hooks/useTrayNavigation";
import { server } from "../msw/server";
import { emitTauriEvent } from "../msw/tauriMocks";
const TAURI_ENDPOINT = "http://tauri.local";
/** 后端留下的导航：取一次就清空（同 `tray::take_tray_navigation`） */
function pendingNavigation(initial: unknown) {
  let pending: unknown = initial;
  server.use(
    http.post(`${TAURI_ENDPOINT}/take_tray_navigation`, () => {
      const value = pending;
      pending = null;
      return HttpResponse.json(value as any);
    }),
  );
  return (next: unknown) => {
    pending = next;
  };
}
describe("parseTrayNavigation", () => {
  it("keeps known apps, sections and intents", () => {
    expect(
      parseTrayNavigation({
        app: "claude",
        intent: "needsRoute",
        providerId: "copilot",
      }),
    ).toEqual({ app: "claude", intent: "needsRoute", providerId: "copilot" });
    expect(parseTrayNavigation({ app: "opencode", intent: "add" })).toEqual({
      app: "opencode",
      intent: "add",
      providerId: undefined,
    });
    expect(parseTrayNavigation({ section: "routing" })).toEqual({
      section: "routing",
    });
  });
  it("drops unknown targets and incomplete needs-route requests", () => {
    expect(parseTrayNavigation(null)).toBeNull();
    expect(parseTrayNavigation({ app: "nope" })).toBeNull();
    expect(parseTrayNavigation({ section: "nope" })).toBeNull();
    expect(parseTrayNavigation({ app: "codex", intent: "needsRoute" })).toEqual(
      { app: "codex" },
    );
  });
});
describe("useTrayNavigation", () => {
  it("serializes takes while another navigation response is pending", async () => {
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/take_tray_navigation`, async () => {
        calls++;
        if (calls === 1) {
          await blocked;
          return HttpResponse.json({ app: "claude" });
        }
        return HttpResponse.json({ app: "codex" });
      }),
    );
    const onNavigate = vi.fn();
    renderHook(() => useTrayNavigation(onNavigate));
    await waitFor(() => expect(calls).toBe(1));
    emitTauriEvent("tray-navigate", null);
    emitTauriEvent("tray-navigate", null);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toBe(1);
    release();
    await waitFor(() => expect(onNavigate).toHaveBeenCalledTimes(2));
    expect(onNavigate.mock.calls.map(([value]) => value.app)).toEqual([
      "claude",
      "codex",
    ]);
    expect(calls).toBe(2);
  });
  it("does not consume queued navigation after unmount", async () => {
    let release!: () => void;
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/take_tray_navigation`, async () => {
        calls++;
        await blocked;
        return HttpResponse.json({ app: "claude" });
      }),
    );
    const onNavigate = vi.fn();
    const { unmount } = renderHook(() => useTrayNavigation(onNavigate));
    await waitFor(() => expect(calls).toBe(1));
    emitTauriEvent("tray-navigate", null);
    unmount();
    release();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(calls).toBe(1);
    expect(onNavigate).not.toHaveBeenCalled();
  });
  it("takes a navigation left before the window existed", async () => {
    pendingNavigation({ app: "codex" });
    const onNavigate = vi.fn();
    renderHook(() => useTrayNavigation(onNavigate));
    await waitFor(() =>
      expect(onNavigate).toHaveBeenCalledWith({
        app: "codex",
        intent: undefined,
        providerId: undefined,
      }),
    );
    expect(onNavigate).toHaveBeenCalledTimes(1);
  });
  it("takes the navigation again on every tray-navigate event", async () => {
    const leave = pendingNavigation(null);
    const onNavigate = vi.fn();
    renderHook(() => useTrayNavigation(onNavigate));
    // 挂载时取过一次（空的），监听已经挂上
    await waitFor(() => expect(onNavigate).not.toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    leave({ app: "claude", intent: "needsRoute", providerId: "copilot" });
    emitTauriEvent("tray-navigate", null);
    await waitFor(() =>
      expect(onNavigate).toHaveBeenCalledWith({
        app: "claude",
        intent: "needsRoute",
        providerId: "copilot",
      }),
    );
  });
});
describe("useTrayAppPageSeen", () => {
  it("reports the app page when shown and when the window regains focus", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/tray_app_page_seen`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json(null);
      }),
    );
    const { rerender } = renderHook(
      ({ app }: { app: "codex" | null }) => useTrayAppPageSeen(app),
      { initialProps: { app: null as "codex" | null } },
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(seen).toEqual([]);
    rerender({ app: "codex" });
    await waitFor(() => expect(seen).toEqual([{ appType: "codex" }]));
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(seen).toHaveLength(2));
  });
});
