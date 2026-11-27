import { backupsApi } from "@/lib/api";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
const BACKUP_LOCATIONS_KEY = ["backup-locations"];
export function useBackupManager() {
  const queryClient = useQueryClient();
  const {
    data: backups = [],
    isLoading,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: ["db-backups"],
    queryFn: () => backupsApi.listDbBackups(),
  });
  // 数据库备份增删也会改变「其他备份」总览里数据库那一行
  const refreshAll = async () => {
    await Promise.all([
      invalidateQueriesAfterChange(queryClient, { queryKey: ["db-backups"] }),
      invalidateQueriesAfterChange(queryClient, {
        queryKey: BACKUP_LOCATIONS_KEY,
      }),
    ]);
  };
  const createMutation = useMutation({
    mutationFn: () => backupsApi.createDbBackup(),
    onSuccess: refreshAll,
  });
  const restoreMutation = useMutation({
    mutationFn: (filename: string) => backupsApi.restoreDbBackup(filename),
    onSuccess: async () => {
      // Invalidate all queries to refresh data from restored database
      await invalidateQueriesAfterChange(queryClient);
    },
  });
  const renameMutation = useMutation({
    mutationFn: ({
      oldFilename,
      newName,
    }: {
      oldFilename: string;
      newName: string;
    }) => backupsApi.renameDbBackup(oldFilename, newName),
    onSuccess: () =>
      invalidateQueriesAfterChange(queryClient, { queryKey: ["db-backups"] }),
  });
  const deleteMutation = useMutation({
    mutationFn: (filename: string) => backupsApi.deleteDbBackup(filename),
    onSuccess: refreshAll,
  });
  return {
    backups,
    isLoading,
    isError,
    error,
    refetch,
    create: createMutation.mutateAsync,
    isCreating: createMutation.isPending,
    restore: restoreMutation.mutateAsync,
    isRestoring: restoreMutation.isPending,
    rename: renameMutation.mutateAsync,
    isRenaming: renameMutation.isPending,
    remove: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
  };
}
export function useBackupLocations() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: BACKUP_LOCATIONS_KEY,
    queryFn: () => backupsApi.listBackupLocations(),
    staleTime: 30000,
  });
  const deleteMutation = useMutation({
    mutationFn: (id: string) => backupsApi.deleteBackupLocation(id),
    onSuccess: async (_freed, id) => {
      await invalidateQueriesAfterChange(queryClient, {
        queryKey: BACKUP_LOCATIONS_KEY,
      });
      // Skills 页的备份列表与这里删的是同一个目录
      if (id === "skills") {
        await invalidateQueriesAfterChange(queryClient, {
          queryKey: ["skills", "backups"],
        });
      }
    },
  });
  return {
    locations: query.data ?? [],
    isLoading: query.isLoading,
    isError: query.isError,
    remove: deleteMutation.mutateAsync,
    isDeleting: deleteMutation.isPending,
  };
}
