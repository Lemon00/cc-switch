import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import DailyMemoryPanel from "@/components/workspace/DailyMemoryPanel";
import {
  workspaceApi,
  type DailyMemorySearchResult,
} from "@/lib/api/workspace";
import { hasUnsavedChanges } from "@/lib/unsavedChanges";

vi.mock("@/lib/api/workspace", () => ({
  workspaceApi: {
    listDailyMemoryFiles: vi.fn(),
    readDailyMemoryFile: vi.fn(),
    deleteDailyMemoryFile: vi.fn(),
    writeDailyMemoryFile: vi.fn(),
    searchDailyMemoryFiles: vi.fn(),
  },
}));
vi.mock("@/components/MarkdownEditor", () => ({
  default: ({
    value,
    onChange,
  }: {
    value: string;
    onChange: (value: string) => void;
  }) => (
    <textarea
      aria-label="Content"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

function deferred() {
  let resolve!: (value: string) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<string>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(workspaceApi.listDailyMemoryFiles).mockResolvedValue([
    {
      filename: "2026-10-10.md",
      date: "2026-10-10",
      sizeBytes: 10,
      modifiedAt: 0,
      preview: "First note",
    },
    {
      filename: "2026-10-11.md",
      date: "2026-10-11",
      sizeBytes: 10,
      modifiedAt: 0,
      preview: "Second note",
    },
  ]);
});

afterEach(() => vi.restoreAllMocks());

describe("DailyMemoryPanel opening", () => {
  it.each(["search", "panel", "unmount"])(
    "invalidates pending searches when closing %s",
    async (mode) => {
      let resolve!: (value: DailyMemorySearchResult[]) => void;
      vi.mocked(workspaceApi.searchDailyMemoryFiles).mockReturnValueOnce(
        new Promise((done) => {
          resolve = done;
        }),
      );
      const view = render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
      await screen.findByText("First note");
      vi.useFakeTimers();
      try {
        fireEvent.click(
          screen.getByRole("button", {
            name: "workspace.dailyMemory.searchScopeHint",
          }),
        );
        fireEvent.change(screen.getByRole("textbox"), {
          target: { value: "old" },
        });
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        if (mode === "search")
          fireEvent.click(
            screen.getByRole("button", {
              name: "workspace.dailyMemory.searchCloseHint",
            }),
          );
        else if (mode === "panel")
          view.rerender(<DailyMemoryPanel isOpen={false} onClose={vi.fn()} />);
        else view.unmount();
        await act(async () =>
          resolve([
            {
              filename: "2026-11-23.md",
              date: "2026-11-23",
              sizeBytes: 1,
              modifiedAt: 0,
              matchCount: 1,
              snippet: "Obsolete result",
            },
          ]),
        );
        if (mode === "panel") {
          await act(async () => {
            view.rerender(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
            await vi.advanceTimersByTimeAsync(300);
          });
        }
        expect(screen.queryByText("Obsolete result")).toBeNull();
      } finally {
        view.unmount();
        vi.useRealTimers();
      }
    },
  );
  it.each(["resolve", "reject"])(
    "ignores an obsolete search %s during the next debounce window",
    async (outcome) => {
      let resolve!: (value: DailyMemorySearchResult[]) => void;
      let reject!: (error: Error) => void;
      const pending = new Promise<DailyMemorySearchResult[]>((done, fail) => {
        resolve = done;
        reject = fail;
      });
      vi.mocked(workspaceApi.searchDailyMemoryFiles)
        .mockReturnValueOnce(pending)
        .mockResolvedValueOnce([
          {
            filename: "2026-10-11.md",
            date: "2026-10-11",
            sizeBytes: 1,
            modifiedAt: 0,
            matchCount: 1,
            snippet: "Newest result",
          },
        ]);
      render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
      await screen.findByText("First note");
      vi.useFakeTimers();
      try {
        fireEvent.click(
          screen.getByRole("button", {
            name: "workspace.dailyMemory.searchScopeHint",
          }),
        );
        fireEvent.change(screen.getByRole("textbox"), {
          target: { value: "old" },
        });
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        fireEvent.change(screen.getByRole("textbox"), {
          target: { value: "new" },
        });
        await act(async () => {
          if (outcome === "resolve") resolve([]);
          else reject(new Error("obsolete"));
        });
        expect(
          screen.getByText("workspace.dailyMemory.searching"),
        ).toBeVisible();
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        expect(screen.getByText("Newest result")).toBeVisible();
        expect(screen.getByRole("textbox")).toHaveValue("new");
      } finally {
        vi.useRealTimers();
      }
    },
  );
  it("preserves drafts after conflicts and sends the original content", async () => {
    vi.mocked(workspaceApi.readDailyMemoryFile).mockResolvedValueOnce(
      "original",
    );
    vi.mocked(workspaceApi.writeDailyMemoryFile).mockRejectedValueOnce(
      new Error("File changed"),
    );
    render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /First note/ }));
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "draft" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "common.save" })),
    );
    expect(workspaceApi.writeDailyMemoryFile).toHaveBeenCalledWith(
      "2026-10-10.md",
      "draft",
      "original",
    );
    expect(screen.getByRole("textbox")).toHaveValue("draft");
    expect(hasUnsavedChanges()).toBe(true);
  });

  it("ignores old save results after closing and selecting another note", async () => {
    const save = deferred();
    vi.mocked(workspaceApi.readDailyMemoryFile)
      .mockResolvedValueOnce("first")
      .mockResolvedValueOnce("second");
    vi.mocked(workspaceApi.writeDailyMemoryFile).mockReturnValueOnce(
      save.promise.then(() => undefined),
    );
    const view = render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /First note/ }));
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "edited" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));
    view.rerender(<DailyMemoryPanel isOpen={false} onClose={vi.fn()} />);
    view.rerender(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /Second note/ }));
    expect(await screen.findByRole("textbox")).toHaveValue("second");
    await act(async () => save.resolve(""));
    expect(hasUnsavedChanges()).toBe(false);
    expect(screen.getByRole("button", { name: "common.save" })).toBeEnabled();
  });

  it.each(["before", "after"])(
    "ignores a pending read that resolves %s deletion completes",
    async (readTiming) => {
      const read = deferred();
      const deletion = deferred();
      vi.mocked(workspaceApi.readDailyMemoryFile).mockReturnValueOnce(
        read.promise,
      );
      vi.mocked(workspaceApi.deleteDailyMemoryFile).mockReturnValueOnce(
        deletion.promise.then(() => undefined),
      );
      render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
      const row = await screen.findByRole("button", { name: /First note/ });
      fireEvent.click(row);
      // The row's trailing delete control stops propagation to openFile.
      fireEvent.click(row.lastElementChild!);
      fireEvent.click(screen.getByRole("button", { name: "common.confirm" }));
      expect(workspaceApi.deleteDailyMemoryFile).toHaveBeenCalledWith(
        "2026-10-10.md",
      );

      if (readTiming === "before") {
        await act(async () => read.resolve("Deleted content"));
        expect(screen.queryByRole("textbox")).toBeNull();
      }
      vi.mocked(workspaceApi.listDailyMemoryFiles).mockResolvedValue([]);
      await act(async () => deletion.resolve(""));
      if (readTiming === "after") {
        await act(async () => read.resolve("Deleted content"));
      }
      expect(screen.queryByRole("textbox")).toBeNull();
      expect(screen.queryByRole("button", { name: "common.save" })).toBeNull();
      expect(screen.queryByText("First note")).toBeNull();
    },
  );

  it("keeps the list visible until the latest selected note is ready", async () => {
    const first = deferred();
    const second = deferred();
    vi.mocked(workspaceApi.readDailyMemoryFile)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /First note/ }));
    expect(screen.getByText("Second note")).toBeVisible();
    expect(screen.queryByRole("textbox")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Second note/ }));
    await act(async () => second.resolve("Second content"));
    expect(screen.getByRole("textbox")).toHaveValue("Second content");
    await act(async () => first.resolve("First content"));
    expect(screen.getByRole("textbox")).toHaveValue("Second content");
  });

  it("ignores a read completed after closing and reopening", async () => {
    const read = deferred();
    vi.mocked(workspaceApi.readDailyMemoryFile).mockReturnValueOnce(
      read.promise,
    );
    const onClose = vi.fn();
    const view = render(<DailyMemoryPanel isOpen onClose={onClose} />);
    fireEvent.click(await screen.findByRole("button", { name: /First note/ }));
    view.rerender(<DailyMemoryPanel isOpen={false} onClose={onClose} />);
    await act(async () => read.resolve("Late content"));
    view.rerender(<DailyMemoryPanel isOpen onClose={onClose} />);
    expect(await screen.findByText("First note")).toBeVisible();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("keeps the list visible when reading fails", async () => {
    const read = deferred();
    vi.mocked(workspaceApi.readDailyMemoryFile).mockReturnValueOnce(
      read.promise,
    );
    render(<DailyMemoryPanel isOpen onClose={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /First note/ }));
    await act(async () => read.reject(new Error("read failed")));
    expect(screen.getByText("First note")).toBeVisible();
    expect(screen.queryByRole("textbox")).toBeNull();
  });
});
