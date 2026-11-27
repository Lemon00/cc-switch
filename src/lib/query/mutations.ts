import { providersApi, sessionsApi, settingsApi, type AppId } from "@/lib/api";
import type { ProviderEditorSave, SwitchResult } from "@/lib/api/providers";
import type { DeleteSessionOptions } from "@/lib/api/sessions";
import { parseLiveEditConflict } from "@/lib/errors/liveEditConflict";
import { sessionKeys } from "@/lib/query/sessions";
import { toast } from "@/lib/toast";
import type { Provider, SessionMeta, Settings } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import { generateUUID } from "@/utils/uuid";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { invalidateQueriesAfterChange } from "./invalidation";
import {
  deleteProviderAndRefresh,
  refreshProviderTray,
  settleProviderChange,
} from "./providerActions";
import { invalidateProviderCaches } from "./providerCache";
export const useAddProviderMutation = (appId: AppId) => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationKey: ["providers", appId, "add"],
    mutationFn: async (
      providerInput: Omit<Provider, "id"> & {
        providerKey?: string;
        addToLive?: boolean;
        editorSave?: ProviderEditorSave;
      },
    ) => {
      const {
        providerKey: _providerKey,
        addToLive,
        editorSave,
        ...rest
      } = providerInput;
      let id: string;
      if (appId === "opencode" || appId === "dsh") {
        if (
          providerInput.category === "omo" ||
          providerInput.category === "omo-slim"
        ) {
          const prefix = providerInput.category === "omo" ? "omo" : "omo-slim";
          id = `${prefix}-${generateUUID()}`;
        } else {
          if (!providerInput.providerKey) {
            throw new Error(`Provider key is required for ${appId}`);
          }
          id = providerInput.providerKey;
        }
      } else {
        id = generateUUID();
      }
      const newProvider: Provider = {
        ...rest,
        id,
        createdAt: Date.now(),
      };
      delete (newProvider as any).providerKey;
      const added = await providersApi.add(
        newProvider,
        appId,
        addToLive,
        editorSave,
      );
      if (!added) {
        throw new Error(
          t("notifications.addFailed", {
            defaultValue: "添加供应商失败: {{error}}",
            error: t("common.unknown"),
          }),
        );
      }
      return newProvider;
    },
    onSuccess: async () => {
      await invalidateProviderCaches(queryClient, appId, "add");
      await refreshProviderTray();
      toast.success(
        t("notifications.providerAdded", {
          defaultValue: "供应商已添加",
        }),
        {
          closeButton: true,
        },
      );
    },
    onError: (error: Error) => {
      // 编辑冲突由对话框让用户选保留哪一边，不弹失败提示。
      if (parseLiveEditConflict(error)) return;
      const rawDetail = extractErrorMessage(error);
      const detail = rawDetail || t("common.unknown");
      toast.error(
        t("notifications.addFailed", {
          defaultValue: "添加供应商失败: {{error}}",
          error: detail,
        }),
      );
    },
    onSettled: () => settleProviderChange(queryClient, appId),
  });
};
export const useUpdateProviderMutation = (appId: AppId) => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationKey: ["providers", appId, "update"],
    mutationFn: async ({
      provider,
      originalId,
      editorSave,
    }: {
      provider: Provider;
      originalId?: string;
      editorSave?: ProviderEditorSave;
    }) => {
      const updated = await providersApi.update(
        provider,
        appId,
        originalId,
        editorSave,
      );
      if (!updated) {
        throw new Error(
          t("notifications.updateFailed", {
            defaultValue: "更新供应商失败: {{error}}",
            error: t("common.unknown"),
          }),
        );
      }
      return provider;
    },
    onSuccess: async (provider, variables) => {
      await invalidateProviderCaches(queryClient, appId, "update", [
        provider.id,
        ...(variables.originalId ? [variables.originalId] : []),
      ]);
      await refreshProviderTray();
      toast.success(
        t("notifications.updateSuccess", {
          defaultValue: "供应商更新成功",
        }),
        {
          closeButton: true,
        },
      );
    },
    onError: (error: Error) => {
      if (parseLiveEditConflict(error)) return;
      const rawDetail = extractErrorMessage(error);
      const detail = rawDetail || t("common.unknown");
      toast.error(
        t("notifications.updateFailed", {
          defaultValue: "更新供应商失败: {{error}}",
          error: detail,
        }),
      );
    },
    onSettled: () => settleProviderChange(queryClient, appId),
  });
};
export const useDeleteProviderMutation = (appId: AppId) => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationKey: ["providers", appId, "delete"],
    mutationFn: async (providerId: string) => {
      const deleted = await deleteProviderAndRefresh(
        queryClient,
        appId,
        providerId,
      );
      if (!deleted) {
        throw new Error(
          t("notifications.deleteFailed", {
            defaultValue: "删除供应商失败: {{error}}",
            error: t("common.unknown"),
          }),
        );
      }
    },
    onSuccess: async () => {
      toast.success(
        t("notifications.deleteSuccess", {
          defaultValue: "供应商已删除",
        }),
        {
          closeButton: true,
        },
      );
    },
    onError: (error: Error) => {
      const rawDetail = extractErrorMessage(error);
      const detail = rawDetail || t("common.unknown");
      toast.error(
        t("notifications.deleteFailed", {
          defaultValue: "删除供应商失败: {{error}}",
          error: detail,
        }),
      );
    },
  });
};
export const useSwitchProviderMutation = (appId: AppId) => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationKey: ["providers", appId, "switch"],
    mutationFn: async (providerId: string): Promise<SwitchResult> => {
      return await providersApi.switch(providerId, appId);
    },
    onSuccess: async () => {
      await invalidateProviderCaches(queryClient, appId, "switch");
      await refreshProviderTray();
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(
        t("notifications.switchFailedTitle", { defaultValue: "切换失败" }),
        {
          description: t("notifications.switchFailed", {
            defaultValue: "切换失败：{{error}}",
            error: detail,
          }),
          duration: 6000,
          action: {
            label: t("common.copy", { defaultValue: "复制" }),
            onClick: () => {
              navigator.clipboard?.writeText(detail).catch(() => undefined);
            },
          },
        },
      );
    },
    onSettled: () => settleProviderChange(queryClient, appId),
  });
};
export const useDeleteSessionMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return useMutation({
    mutationFn: async (input: DeleteSessionOptions) => {
      const deleted = await sessionsApi.delete(input);
      if (!deleted) {
        throw new Error(
          t("sessionManager.deleteNotFound", {
            defaultValue: "会话未删除（可能已不存在）",
          }),
        );
      }
      return input;
    },
    onSuccess: async (input) => {
      await queryClient.cancelQueries({ queryKey: ["sessions"] });
      const deleted = queryClient
        .getQueryData<SessionMeta[]>(["sessions"])
        ?.find(
          (session) =>
            session.providerId === input.providerId &&
            session.sessionId === input.sessionId &&
            session.sourcePath === input.sourcePath,
        );
      const deletedTitle = deleted?.title?.trim() || deleted?.summary?.trim();
      queryClient.setQueryData<SessionMeta[]>(["sessions"], (current) =>
        (current ?? []).filter(
          (session) =>
            !(
              session.providerId === input.providerId &&
              session.sessionId === input.sessionId &&
              session.sourcePath === input.sourcePath
            ),
        ),
      );
      queryClient.removeQueries({
        queryKey: sessionKeys.messages(input.providerId, input.sourcePath),
      });
      queryClient.removeQueries({
        queryKey: sessionKeys.transcript(input.providerId, input.sourcePath),
      });
      queryClient.removeQueries({
        queryKey: sessionKeys.blockContents(input.providerId, input.sourcePath),
      });
      await invalidateQueriesAfterChange(queryClient, {
        queryKey: ["sessions"],
      });
      toast.success(
        deletedTitle
          ? t("sessionManager.sessionDeletedNamed", { title: deletedTitle })
          : t("sessionManager.sessionDeleted", {
              defaultValue: "会话已删除",
            }),
      );
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(
        t("sessionManager.deleteFailed", {
          defaultValue: "删除会话失败: {{error}}",
          error: detail,
        }),
      );
    },
  });
};
export const useSaveSettingsMutation = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (settings: Partial<Settings>) => {
      await settingsApi.save(settings);
    },
    onSettled: async () => {
      await invalidateQueriesAfterChange(queryClient, {
        queryKey: ["settings"],
      });
      await invalidateQueriesAfterChange(queryClient, {
        queryKey: ["opencode", "runtime-models"],
      });
    },
  });
};
