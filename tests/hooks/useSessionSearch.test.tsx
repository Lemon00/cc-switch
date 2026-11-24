import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useSessionSearch } from "@/hooks/useSessionSearch";
import type { SessionMeta } from "@/types";

const session = (overrides: Partial<SessionMeta> = {}): SessionMeta => ({
  providerId: "claude",
  sessionId: "abcdef-123456-789abc",
  title: "Café 项目功能性能审查",
  summary: "检查连续中文中间的任意子串以及边界情况",
  projectDir: "C:\\Work\\MyProject",
  sourcePath: "/sessions/transcript.jsonl",
  ...overrides,
});

describe("useSessionSearch", () => {
  it("matches Chinese infixes, case-insensitive paths, IDs and multiple words", () => {
    const entry = session();
    const { result } = renderHook(() =>
      useSessionSearch({ sessions: [entry], providerFilter: "all" }),
    );
    for (const query of [
      "功能性能",
      "中间的任意",
      "myproject",
      "23456",
      "cafe 边界",
      "C:\\Work",
      "transcript.jsonl",
    ]) {
      expect(result.current.search(query), query).toEqual([entry]);
    }
    expect(result.current.search("missing")).toEqual([]);
    expect(result.current.search("!!!")).toEqual([]);
  });

  it("filters providers, updates metadata and sorts empty searches without mutating input", () => {
    const entries = [
      session({ createdAt: 1 }),
      session({ providerId: "codex", lastActiveAt: 3 }),
    ];
    const { result, rerender } = renderHook(
      (props) => useSessionSearch(props),
      { initialProps: { sessions: entries, providerFilter: "all" } },
    );
    expect(result.current.search(" ")).toEqual([entries[1], entries[0]]);
    expect(entries[0].providerId).toBe("claude");
    rerender({ sessions: entries, providerFilter: "codex" });
    expect(result.current.search("边界")).toEqual([entries[1]]);
    const updated = session({
      title: "changed",
      summary: "",
      providerId: "codex",
    });
    rerender({ sessions: [updated], providerFilter: "codex" });
    expect(result.current.search("边界")).toEqual([]);
    expect(result.current.search("changed")).toEqual([updated]);
  });

  it("does not read searchable metadata until a non-empty query", () => {
    const entry = session();
    let reads = 0;
    Object.defineProperty(entry, "summary", {
      get: () => {
        reads += 1;
        return "中文".repeat(10000);
      },
    });
    const { result } = renderHook(() =>
      useSessionSearch({ sessions: [entry], providerFilter: "all" }),
    );
    result.current.search("");
    expect(reads).toBe(0);
    expect(result.current.search("中文")).toEqual([entry]);
    result.current.search("不存在");
    expect(reads).toBe(1);
  });
});
