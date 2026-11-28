import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDraftEditorProjection } from "@/components/providers/forms/hooks/useDraftEditorProjection";
import type { ProviderMeta } from "@/types";
const accountMeta: ProviderMeta = { providerType: "codex_oauth" };
const getEditorView = vi.fn();
const toastError = vi.fn();

vi.mock("@/lib/api", () => ({
  providersApi: {
    getEditorView: (...args: unknown[]) => getEditorView(...args),
  },
}));
vi.mock("sonner", () => ({
  toast: { error: (...args: unknown[]) => toastError(...args) },
}));
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
};
describe("useDraftEditorProjection", () => {
  beforeEach(() => {
    getEditorView.mockReset();
    toastError.mockReset();
  });
  it("连续切换预设时只认最后一次投影", async () => {
    const first = deferred<{
      settings: Record<string, unknown>;
    }>();
    const second = deferred<{
      settings: Record<string, unknown>;
    }>();
    getEditorView
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const onBase = vi.fn();
    const apply = vi.fn();
    const { result } = renderHook(() =>
      useDraftEditorProjection("codex", onBase),
    );
    act(() => {
      result.current.projectDraft(
        { config: "a" },
        undefined,
        apply,
        accountMeta,
      );
      result.current.projectDraft({ config: "b" }, "official", apply);
    });
    expect(getEditorView).toHaveBeenNthCalledWith(
      1,
      "codex",
      { config: "a" },
      undefined,
      undefined,
      accountMeta,
    );
    expect(getEditorView).toHaveBeenLastCalledWith(
      "codex",
      { config: "b" },
      "official",
      undefined,
      undefined,
    );
    await act(async () => {
      second.resolve({ settings: { config: "B" } });
      await second.promise;
    });
    await act(async () => {
      first.resolve({ settings: { config: "A" } });
      await first.promise;
    });
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith({ config: "B" });
    // 底和投影成它的草稿一起交出去，保存时后端按草稿分开预设带的字段。
    expect(onBase).toHaveBeenLastCalledWith({ config: "B" }, { config: "b" });
  });
  it("没有接收方时不发请求（编辑对话框等场景）", () => {
    const { result } = renderHook(() => useDraftEditorProjection("codex"));
    act(() => {
      result.current.projectDraft({ config: "x" }, undefined, vi.fn());
    });
    expect(getEditorView).not.toHaveBeenCalled();
  });
});
