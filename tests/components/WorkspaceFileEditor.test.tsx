import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import WorkspaceFileEditor from "@/components/workspace/WorkspaceFileEditor";
import { workspaceApi } from "@/lib/api/workspace";
import { hasUnsavedChanges } from "@/lib/unsavedChanges";

vi.mock("@/lib/api/workspace", () => ({
  workspaceApi: { readFile: vi.fn(), writeFile: vi.fn() },
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

describe("WorkspaceFileEditor opening", () => {
  it("does not apply an old save result to a newly opened file", async () => {
    const save = deferred();
    vi.mocked(workspaceApi.readFile)
      .mockResolvedValueOnce("A original")
      .mockResolvedValueOnce("B original");
    vi.mocked(workspaceApi.writeFile).mockReturnValueOnce(
      save.promise.then(() => undefined),
    );
    const view = render(
      <WorkspaceFileEditor isOpen filename="A.md" onClose={vi.fn()} />,
    );
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "A edited" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));
    view.rerender(
      <WorkspaceFileEditor isOpen filename="B.md" onClose={vi.fn()} />,
    );
    expect(await screen.findByRole("textbox")).toHaveValue("B original");
    await act(async () => save.resolve(""));
    expect(hasUnsavedChanges()).toBe(false);
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "B edited" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "common.save" })),
    );
    expect(workspaceApi.writeFile).toHaveBeenLastCalledWith(
      "B.md",
      "B edited",
      "B original",
    );
  });

  it("keeps edits made during an in-flight save dirty", async () => {
    const save = deferred();
    vi.mocked(workspaceApi.readFile).mockResolvedValueOnce("original");
    vi.mocked(workspaceApi.writeFile).mockReturnValueOnce(
      save.promise.then(() => undefined),
    );
    render(
      <WorkspaceFileEditor isOpen filename="MEMORY.md" onClose={vi.fn()} />,
    );
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "submitted" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.save" }));
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "new draft" },
    });
    await act(async () => save.resolve(""));
    expect(screen.getByRole("textbox")).toHaveValue("new draft");
    expect(hasUnsavedChanges()).toBe(true);
  });

  it("tracks drafts and sends the original content as a save precondition", async () => {
    vi.mocked(workspaceApi.readFile).mockResolvedValueOnce("original");
    vi.mocked(workspaceApi.writeFile).mockResolvedValueOnce(undefined);
    const onClose = vi.fn();
    render(
      <WorkspaceFileEditor isOpen filename="MEMORY.md" onClose={onClose} />,
    );
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "edited" },
    });
    expect(hasUnsavedChanges()).toBe(true);
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "common.save" })),
    );
    expect(workspaceApi.writeFile).toHaveBeenCalledWith(
      "MEMORY.md",
      "edited",
      "original",
    );
    expect(hasUnsavedChanges()).toBe(false);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("preserves a draft and dirty state when the file changed externally", async () => {
    vi.mocked(workspaceApi.readFile).mockResolvedValueOnce("original");
    vi.mocked(workspaceApi.writeFile).mockRejectedValueOnce(
      new Error("File changed"),
    );
    render(
      <WorkspaceFileEditor isOpen filename="MEMORY.md" onClose={vi.fn()} />,
    );
    fireEvent.change(await screen.findByRole("textbox"), {
      target: { value: "draft" },
    });
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "common.save" })),
    );
    expect(screen.getByRole("textbox")).toHaveValue("draft");
    expect(hasUnsavedChanges()).toBe(true);
  });

  it("waits for content and ignores an older file read", async () => {
    const first = deferred();
    const second = deferred();
    vi.mocked(workspaceApi.readFile)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const onClose = vi.fn();
    const view = render(
      <WorkspaceFileEditor isOpen filename="A.md" onClose={onClose} />,
    );
    expect(screen.queryByRole("heading")).toBeNull();
    expect(screen.queryByText("prompts.loading")).toBeNull();
    view.rerender(
      <WorkspaceFileEditor isOpen filename="B.md" onClose={onClose} />,
    );
    await act(async () => second.resolve("B content"));
    expect(screen.getByRole("textbox")).toHaveValue("B content");
    expect(screen.getByRole("textbox").closest(".bg-app")).toHaveStyle({
      opacity: "1",
    });
    await act(async () => first.resolve("A content"));
    expect(screen.getByRole("textbox")).toHaveValue("B content");
  });

  it("does not reopen after closing during a read", async () => {
    const read = deferred();
    vi.mocked(workspaceApi.readFile).mockReturnValueOnce(read.promise);
    const onClose = vi.fn();
    const view = render(
      <WorkspaceFileEditor isOpen filename="A.md" onClose={onClose} />,
    );
    view.rerender(
      <WorkspaceFileEditor isOpen={false} filename="A.md" onClose={onClose} />,
    );
    await act(async () => read.resolve("late content"));
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("returns to the list on failure instead of opening an empty editor", async () => {
    const read = deferred();
    vi.mocked(workspaceApi.readFile).mockReturnValueOnce(read.promise);
    const onClose = vi.fn();
    render(<WorkspaceFileEditor isOpen filename="A.md" onClose={onClose} />);
    await act(async () => read.reject(new Error("read failed")));
    expect(onClose).toHaveBeenCalledOnce();
    expect(screen.queryByRole("textbox")).toBeNull();
  });
});
