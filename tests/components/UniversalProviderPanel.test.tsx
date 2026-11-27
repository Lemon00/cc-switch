import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UniversalProvider } from "@/types";
import { UniversalProviderPanel } from "@/components/universal/UniversalProviderPanel";
const api = vi.hoisted(() => ({
  getAll: vi.fn(),
  upsert: vi.fn(),
  sync: vi.fn(),
  delete: vi.fn(),
}));
const notify = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
vi.mock("@/lib/api", () => ({ universalProvidersApi: api }));
vi.mock("@/lib/toast", () => ({ toast: notify }));
vi.mock("@/components/universal/UniversalProviderCard", () => ({
  UniversalProviderCard: ({
    provider,
    onEdit,
    onSync,
    onDelete,
    onDuplicate,
  }: any) => (
    <article>
      <span>{provider.name}</span>
      <button onClick={() => onEdit(provider)}>edit</button>
      <button onClick={() => onSync(provider.id)}>sync</button>
      <button onClick={() => onDelete(provider.id)}>delete</button>
      <button onClick={() => onDuplicate(provider)}>duplicate</button>
    </article>
  ),
}));
vi.mock("@/components/universal/UniversalProviderFormModal", () => ({
  UniversalProviderFormModal: ({
    isOpen,
    editingProvider,
    onSaveAndSync,
  }: any) =>
    isOpen ? (
      <button
        onClick={() => onSaveAndSync({ ...editingProvider, name: "Updated" })}
      >
        save
      </button>
    ) : null,
}));
vi.mock("@/components/ConfirmDialog", () => ({
  ConfirmDialog: ({ isOpen, onConfirm, pending }: any) =>
    isOpen ? (
      <button aria-busy={pending} onClick={onConfirm}>
        confirm
      </button>
    ) : null,
}));
const provider: UniversalProvider = {
  id: "universal",
  name: "Original",
  providerType: "custom",
  apps: { claude: true, codex: true },
  baseUrl: "https://example.com",
  apiKey: "key",
  models: {},
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
describe("UniversalProviderPanel write consistency", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getAll.mockReset().mockResolvedValue({ universal: provider });
    api.upsert.mockReset().mockResolvedValue(true);
    api.sync.mockReset().mockResolvedValue(true);
    api.delete.mockReset().mockResolvedValue(true);
  });
  it("reloads the saved parent and shows the backend detail after partial sync failure", async () => {
    render(<UniversalProviderPanel />);
    await screen.findByText("Original");
    api.getAll.mockResolvedValue({
      universal: { ...provider, name: "Updated" },
    });
    api.sync.mockRejectedValueOnce(
      new Error("Codex config could not be written"),
    );
    fireEvent.click(screen.getByText("edit"));
    fireEvent.click(screen.getByText("save"));
    await screen.findByText("Updated");
    expect(notify.error).toHaveBeenCalledWith(
      "Codex config could not be written",
    );
    expect(notify.success).not.toHaveBeenCalled();
  });
  it("keeps a partially saved duplicate visible so it can be retried", async () => {
    render(<UniversalProviderPanel />);
    await screen.findByText("Original");
    api.sync.mockRejectedValueOnce(new Error("sync failed"));
    api.getAll.mockResolvedValue({
      universal: provider,
      copy: { ...provider, id: "copy", name: "Saved copy" },
    });
    fireEvent.click(screen.getByText("duplicate"));
    await screen.findByText("Saved copy");
  });
  it("ignores an older list response after a later refresh completes", async () => {
    render(<UniversalProviderPanel />);
    await screen.findByText("Original");
    const older = deferred<Record<string, UniversalProvider>>();
    const newer = deferred<Record<string, UniversalProvider>>();
    api.getAll
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);
    act(() => {
      fireEvent.click(screen.getByText("duplicate"));
      fireEvent.click(screen.getByText("duplicate"));
    });
    await waitFor(() => expect(api.getAll).toHaveBeenCalledTimes(3));
    await act(async () =>
      newer.resolve({ universal: { ...provider, name: "Newest" } }),
    );
    await act(async () => older.resolve({ universal: provider }));
    expect(screen.getByText("Newest")).toBeInTheDocument();
    expect(screen.queryByText("Original")).not.toBeInTheDocument();
  });
  it.each(["sync", "delete"] as const)(
    "blocks duplicate %s confirmation while pending",
    async (operation) => {
      render(<UniversalProviderPanel />);
      await screen.findByText("Original");
      const pending = deferred<boolean>();
      api[operation].mockReturnValueOnce(pending.promise);
      fireEvent.click(screen.getByText(operation));
      act(() => {
        fireEvent.click(screen.getByText("confirm"));
        fireEvent.click(screen.getByText("confirm"));
      });
      expect(api[operation]).toHaveBeenCalledOnce();
      await act(async () => pending.resolve(true));
      expect(screen.queryByText("confirm")).not.toBeInTheDocument();
    },
  );
});
