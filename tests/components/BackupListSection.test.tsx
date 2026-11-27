import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BackupListSection } from "@/components/settings/BackupListSection";
const mocks = vi.hoisted(() => ({
  restore: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  error: vi.fn(),
  loadError: null as Error | null,
  refetch: vi.fn(),
}));
vi.mock("@/lib/toast", () => ({ toast: mocks }));
vi.mock("@/hooks/useBackupManager", () => ({
  useBackupManager: () => ({
    backups: [
      {
        filename: "selected.db",
        sizeBytes: 20,
        createdAt: "2026-11-23T00:00:00Z",
      },
    ],
    isLoading: false,
    isError: !!mocks.loadError,
    error: mocks.loadError,
    refetch: mocks.refetch,
    restore: mocks.restore,
    create: vi.fn(),
    rename: vi.fn(),
    remove: vi.fn(),
  }),
}));
vi.mock("@/components/settings/BackupStorageSection", () => ({
  BackupStorageSection: () => null,
  formatBytes: (bytes: number) => String(bytes),
}));
describe("BackupListSection restore result", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.loadError = null;
  });
  it("shows list failures and provides a retry instead of reporting an empty list", () => {
    mocks.loadError = new Error("Cannot read backup directory");
    render(<BackupListSection onSettingsChange={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Cannot read backup directory",
    );
    fireEvent.click(within(screen.getByRole("alert")).getByRole("button"));
    expect(mocks.refetch).toHaveBeenCalledOnce();
    expect(screen.queryByText("No backups yet")).toBeNull();
  });
  it.each([undefined, "Database restored, but Prompt projection failed"])(
    "shows the correct outcome when warning is %s",
    async (warning) => {
      mocks.restore.mockResolvedValue({ backupId: "safety.db", warning });
      render(<BackupListSection onSettingsChange={vi.fn()} />);
      fireEvent.click(screen.getByRole("button", { name: "Restore" }));
      const dialog = screen.getByRole("dialog");
      fireEvent.click(
        within(dialog).getByRole("button", {
          name: "Restore",
        }),
      );
      await waitFor(() =>
        expect(mocks.restore).toHaveBeenCalledWith("selected.db"),
      );
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      if (warning) {
        expect(mocks.warning).toHaveBeenCalledWith(warning, expect.any(Object));
        expect(mocks.success).not.toHaveBeenCalled();
      } else {
        expect(mocks.success).toHaveBeenCalledWith(
          "Restore successful! Safety backup created",
          expect.objectContaining({
            description: expect.stringContaining("safety.db"),
          }),
        );
        expect(mocks.warning).not.toHaveBeenCalled();
      }
      expect(mocks.error).not.toHaveBeenCalled();
    },
  );
});
