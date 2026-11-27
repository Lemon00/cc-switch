import type { PropsWithChildren } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import {
  QueryClient,
  QueryClientProvider,
  QueryObserver,
} from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBackupLocations, useBackupManager } from "@/hooks/useBackupManager";
import { backupsApi } from "@/lib/api";
vi.mock("@/lib/api", () => ({
  backupsApi: {
    listDbBackups: vi.fn().mockResolvedValue([]),
    createDbBackup: vi.fn().mockResolvedValue("new.db"),
    deleteDbBackup: vi.fn().mockResolvedValue(undefined),
    renameDbBackup: vi.fn().mockResolvedValue("new.db"),
    listBackupLocations: vi.fn().mockResolvedValue([]),
    deleteBackupLocation: vi.fn().mockResolvedValue(1),
    restoreDbBackup: vi
      .fn()
      .mockResolvedValue({ backupId: "before-restore.db" }),
  },
}));
const clients: QueryClient[] = [];
beforeEach(() => {
  vi.mocked(backupsApi.listDbBackups).mockReset().mockResolvedValue([]);
  vi.mocked(backupsApi.listBackupLocations).mockReset().mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  clients.splice(0).forEach((client) => client.clear());
});
describe("backup restoration", () => {
  it.each(["create", "remove", "rename"] as const)(
    "%s discards an initial backup listing that predates the write",
    async (operation) => {
      const client = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      clients.push(client);
      let finishOld!: (value: never[]) => void;
      const saved = [
        { filename: "new.db", sizeBytes: 1, createdAt: "2026-11-26T00:00:00Z" },
      ];
      vi.mocked(backupsApi.listDbBackups)
        .mockImplementationOnce(
          () =>
            new Promise((resolve) => {
              finishOld = resolve;
            }),
        )
        .mockResolvedValue(saved);
      const { result } = renderHook(useBackupManager, {
        wrapper: ({ children }: PropsWithChildren) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      });
      let pending!: Promise<unknown>;
      await act(async () => {
        pending =
          operation === "create"
            ? result.current.create()
            : operation === "remove"
              ? result.current.remove("old.db")
              : result.current.rename({
                  oldFilename: "old.db",
                  newName: "new",
                });
      });
      await act(async () => {
        finishOld([]);
        await pending;
      });
      expect(client.getQueryData(["db-backups"])).toEqual(saved);
      expect(backupsApi.listDbBackups).toHaveBeenCalledTimes(2);
    },
  );
  it("clears stale Skills backup and storage overview reads after deleting the directory", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    clients.push(client);
    let finishOverview!: (value: never[]) => void;
    vi.mocked(backupsApi.listBackupLocations).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishOverview = resolve;
        }),
    );
    let finishSkills!: (value: string[]) => void;
    const skillRead = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<string[]>((resolve) => {
            finishSkills = resolve;
          }),
      )
      .mockResolvedValue([]);
    const observer = new QueryObserver(client, {
      queryKey: ["skills", "backups"],
      queryFn: skillRead,
    });
    const off = observer.subscribe(() => {});
    const { result } = renderHook(useBackupLocations, {
      wrapper: ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      ),
    });
    let pending!: Promise<unknown>;
    await act(async () => {
      pending = result.current.remove("skills");
    });
    await act(async () => {
      finishOverview([]);
      finishSkills(["deleted-backup"]);
      await pending;
    });
    expect(client.getQueryData(["skills", "backups"])).toEqual([]);
    expect(backupsApi.listBackupLocations).toHaveBeenCalledTimes(2);
    expect(skillRead).toHaveBeenCalledTimes(2);
    off();
  });
  it("refreshes active cold queries and invalidates inactive data after replacing the database", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    clients.push(client);
    const key = ["providers", "claude"];
    let finishOld!: (value: string) => void;
    let reads = 0;
    const observer = new QueryObserver(client, {
      queryKey: key,
      queryFn: () =>
        ++reads === 1
          ? new Promise<string>((resolve) => {
              finishOld = resolve;
            })
          : Promise.resolve("restored"),
    });
    const off = observer.subscribe(() => {});
    client.setQueryData(["prompts"], "before-restore");
    const { result } = renderHook(useBackupManager, {
      wrapper: ({ children }: PropsWithChildren) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      ),
    });
    await act(async () => {
      const restoring = result.current.restore("selected.db");
      // The old IPC response arrives after the backend restoration has finished.
      const oldReply = new Promise<void>((resolve) => {
        setTimeout(() => {
          finishOld("before-restore");
          resolve();
        }, 0);
      });
      await Promise.all([restoring, oldReply]);
    });
    expect(client.getQueryData(key)).toBe("restored");
    expect(client.getQueryState(["prompts"])?.isInvalidated).toBe(true);
    expect(reads).toBe(2);
    off();
  });
});
