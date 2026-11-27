import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { parse } from "smol-toml";
import { UniversalProviderFormModal } from "@/components/universal/UniversalProviderFormModal";
import {
  createUniversalProviderFromPreset,
  universalProviderPresets,
} from "@/config/universalProviderPresets";
import type { UniversalProvider } from "@/types";
vi.mock("@/components/JsonEditor", () => ({
  default: ({ value }: { value: string }) => (
    <pre data-testid="preview">{value}</pre>
  ),
}));
vi.mock("@/components/ProviderIcon", () => ({ ProviderIcon: () => null }));
vi.mock("@/hooks/useDarkMode", () => ({ useDarkMode: () => false }));
vi.mock("@/components/common/FullScreenPanel", () => ({
  FullScreenPanel: ({
    isOpen,
    children,
    footer,
    onClose,
  }: {
    isOpen: boolean;
    children: ReactNode;
    footer: ReactNode;
    onClose: () => void;
  }) =>
    isOpen ? (
      <div>
        <button onClick={onClose}>back</button>
        {children}
        {footer}
      </div>
    ) : null,
}));
vi.mock("@/components/ConfirmDialog", () => ({
  ConfirmDialog: ({
    isOpen,
    onConfirm,
    pending,
  }: {
    isOpen: boolean;
    onConfirm: () => void;
    pending: boolean;
  }) =>
    isOpen ? (
      <button disabled={pending} onClick={onConfirm}>
        confirm-sync
      </button>
    ) : null,
}));
const provider = () =>
  createUniversalProviderFromPreset(
    universalProviderPresets[0],
    "existing",
    "https://example.test/custom",
    "key",
    "Gateway",
  );
describe("UniversalProviderFormModal", () => {
  it("waits for saving, prevents duplicate submissions, and retains a failed draft", async () => {
    let finish!: (result: boolean) => void;
    const onSave = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          finish = resolve;
        }),
    );
    const onClose = vi.fn();
    render(
      <UniversalProviderFormModal
        isOpen
        onClose={onClose}
        onSave={onSave}
        editingProvider={provider()}
      />,
    );
    const submit = screen.getByRole("button", { name: "添加" });
    fireEvent.click(submit);
    fireEvent.click(submit);
    fireEvent.click(screen.getByText("back"));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
    expect(submit).toBeDisabled();
    await act(async () => finish(false));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByDisplayValue("Gateway")).toBeInTheDocument();
    fireEvent.click(submit);
    await act(async () => finish(true));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
  it("uses one ID when retrying a new draft after a partial save", async () => {
    const saves: UniversalProvider[] = [];
    const onSave = vi.fn(async (value: UniversalProvider) => {
      saves.push(value);
      return false;
    });
    render(
      <UniversalProviderFormModal isOpen onClose={vi.fn()} onSave={onSave} />,
    );
    fireEvent.change(screen.getByPlaceholderText("https://api.example.com"), {
      target: { value: "https://example.test" },
    });
    fireEvent.change(screen.getByPlaceholderText("sk-..."), {
      target: { value: "key" },
    });
    const submit = screen.getByRole("button", { name: "添加" });
    fireEvent.click(submit);
    await waitFor(() => expect(submit).not.toBeDisabled());
    fireEvent.click(submit);
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(2));
    expect(saves[0].id).toBe(saves[1].id);
  });
  it("keeps the editor open when save-and-sync rejects", async () => {
    const onClose = vi.fn();
    render(
      <UniversalProviderFormModal
        isOpen
        onClose={onClose}
        onSave={vi.fn()}
        onSaveAndSync={vi.fn().mockRejectedValue(new Error("failed"))}
        editingProvider={provider()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "保存并同步" }));
    fireEvent.click(screen.getByText("confirm-sync"));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
  it("ignores save completion after the editor unmounts", async () => {
    let finish!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const onClose = vi.fn();
    const { unmount } = render(
      <UniversalProviderFormModal
        isOpen
        onClose={onClose}
        onSave={onSave}
        editingProvider={provider()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    unmount();
    await act(async () => finish());
    expect(onClose).not.toHaveBeenCalled();
  });
  it("does not close a reopened editor when an earlier save finishes", async () => {
    let finish!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const onClose = vi.fn();
    const editing = provider();
    const props = { onClose, onSave, editingProvider: editing };
    const { rerender } = render(
      <UniversalProviderFormModal isOpen {...props} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    rerender(<UniversalProviderFormModal isOpen={false} {...props} />);
    rerender(<UniversalProviderFormModal isOpen {...props} />);
    await act(async () => finish());
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "添加" })).not.toBeDisabled();
  });
  it("previews explicitly empty model fields without substituting defaults", () => {
    const editing = provider();
    editing.models.claude = { model: "" };
    editing.models.gemini = { model: "" };
    render(
      <UniversalProviderFormModal
        isOpen
        onClose={vi.fn()}
        onSave={vi.fn()}
        editingProvider={editing}
      />,
    );
    const previews = screen
      .getAllByTestId("preview")
      .map((node) => JSON.parse(node.textContent!));
    expect(
      previews.find((value) => value.env?.ANTHROPIC_MODEL !== undefined).env
        .ANTHROPIC_MODEL,
    ).toBe("");
    expect(
      previews.find((value) => value.env?.GEMINI_MODEL !== undefined),
    ).toBeUndefined();
  });
  it("previews escaped TOML with the actual custom endpoint", () => {
    const editing = provider();
    editing.models.codex = { model: 'model"\nquoted', reasoningEffort: "high" };
    render(
      <UniversalProviderFormModal
        isOpen
        onClose={vi.fn()}
        onSave={vi.fn()}
        editingProvider={editing}
      />,
    );
    const previews = screen
      .getAllByTestId("preview")
      .map((node) => JSON.parse(node.textContent!));
    const config = parse(previews.find((value) => value.config).config);
    expect(config.model).toBe('model"\nquoted');
    expect(config.model_providers).toEqual({
      custom: {
        name: "NewAPI",
        base_url: editing.baseUrl,
        wire_api: "responses",
        requires_openai_auth: true,
      },
    });
  });
});
