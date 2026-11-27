import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import PromptFormPanel from "@/components/prompts/PromptFormPanel";
import type { Prompt } from "@/lib/api";
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options?.file ? `${key}:${String(options.file)}` : key,
  }),
}));
const active: Prompt = {
  id: "team",
  name: "Team rules",
  content: "# Rules",
  enabled: true,
  createdAt: 1,
  updatedAt: 1,
};
describe("PromptFormPanel", () => {
  it("submits once and refuses to close while saving", async () => {
    let resolveSave!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        }),
    );
    const onClose = vi.fn();
    render(
      <PromptFormPanel appId="claude" onSave={onSave} onClose={onClose} />,
    );
    const nameInput = screen.getByLabelText(/prompts.name/);
    fireEvent.change(nameInput, { target: { value: "My Prompt" } });
    const saveButton = screen.getByRole("button", {
      name: "prompts.addSubmit",
    });
    fireEvent.click(saveButton);
    fireEvent.click(saveButton);
    fireEvent.click(screen.getByRole("button", { name: "common.cancel" }));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
    expect(nameInput).toBeDisabled();
    expect(screen.getByLabelText("prompts.content")).toBeDisabled();
    await act(async () => {
      resolveSave();
      await Promise.resolve();
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });
  it("stays open when the parent write lock rejects a save", async () => {
    const onSave = vi.fn().mockResolvedValue(false);
    const onClose = vi.fn();
    render(<PromptFormPanel appId="codex" onSave={onSave} onClose={onClose} />);
    fireEvent.change(screen.getByLabelText(/prompts.name/), {
      target: { value: "Codex Prompt" },
    });
    fireEvent.click(screen.getByRole("button", { name: "prompts.addSubmit" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/prompts.name/)).toBeEnabled();
  });
  it("preserves whitespace and the original snapshot across background refreshes", async () => {
    const onSave = vi.fn().mockResolvedValue(false);
    const props = {
      appId: "codex" as const,
      editingId: "team",
      initialData: active,
      fileContent: active.content,
      onSave,
      onClose: vi.fn(),
    };
    const view = render(<PromptFormPanel {...props} />);
    fireEvent.change(screen.getByLabelText("prompts.content"), {
      target: { value: "  draft\n" },
    });
    view.rerender(
      <PromptFormPanel
        {...props}
        initialData={{ ...active, content: "external" }}
        fileContent="external"
      />,
    );
    expect(screen.getByLabelText("prompts.content")).toHaveValue("  draft\n");
    fireEvent.click(
      screen.getByRole("button", {
        name: "prompts.saveAndOverwrite:AGENTS.md",
      }),
    );
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        "team",
        expect.objectContaining({ content: "  draft\n" }),
        { prompt: active, fileContent: active.content },
      ),
    );
  });
  it("keeps a dirty form open until discarding is confirmed", () => {
    const onClose = vi.fn();
    render(
      <PromptFormPanel appId="claude" onSave={vi.fn()} onClose={onClose} />,
    );
    fireEvent.change(screen.getByLabelText(/prompts.name/), {
      target: { value: "draft" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.cancel" }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "common.unsavedLeaveConfirm" }),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
