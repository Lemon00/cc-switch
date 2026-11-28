import { injectCodingPlanUsageScript } from "@/config/codingPlanProviders";
import { providersApi, settingsApi, type AppId } from "@/lib/api";
import type { ProviderEditorSave } from "@/lib/api/providers";
import { logFrontendInfo } from "@/lib/frontendLogger";
import { getOpencodeDeclaredModels } from "@/lib/providerConfig";
import {
  useAddProviderMutation,
  useDeleteProviderMutation,
  useSwitchProviderMutation,
  useUpdateProviderMutation,
} from "@/lib/query";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import { usageKeys } from "@/lib/query/usage";
import { toast } from "@/lib/toast";
import type { Provider, UsageScript } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import { supportsOfficialProxyTakeover } from "@/utils/providerCapabilities";
import { routeSwitchNotification } from "@/utils/routeSwitchNotification";
import { getRoutingReason } from "@/utils/routingReason";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
/**
 * Hook for managing provider actions (add, update, delete, switch)
 * Extracts business logic from App.tsx
 */
export function useProviderActions(
  activeApp: AppId,
  isProxyTakeover?: boolean,
) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const addProviderMutation = useAddProviderMutation(activeApp);
  const updateProviderMutation = useUpdateProviderMutation(activeApp);
  const deleteProviderMutation = useDeleteProviderMutation(activeApp);
  const switchProviderMutation = useSwitchProviderMutation(activeApp);
  // Claude 插件同步逻辑
  const syncClaudePlugin = useCallback(
    async (provider: Provider) => {
      if (activeApp !== "claude") return;
      try {
        const settings = await settingsApi.get();
        if (!settings?.enableClaudePluginIntegration) {
          return;
        }
        const isOfficial = provider.category === "official";
        await settingsApi.applyClaudePluginConfig({ official: isOfficial });
        // 静默执行，不显示成功通知
      } catch (error) {
        const detail =
          extractErrorMessage(error) ||
          t("notifications.syncClaudePluginFailed", {
            defaultValue: "同步 Claude 插件失败",
          });
        toast.error(detail, { duration: 4200 });
      }
    },
    [activeApp, t],
  );
  // 添加供应商
  const addProvider = useCallback(
    async (
      provider: Omit<Provider, "id"> & {
        providerKey?: string;
        suggestedDefaults?: never;
        addToLive?: boolean;
        editorSave?: ProviderEditorSave;
      },
    ) => {
      const enhanced = injectCodingPlanUsageScript(activeApp, provider);
      await addProviderMutation.mutateAsync(enhanced);
    },
    [addProviderMutation, activeApp, queryClient, t],
  );
  // 更新供应商
  const updateProvider = useCallback(
    async (
      provider: Provider,
      originalId?: string,
      editorSave?: ProviderEditorSave,
    ) => {
      await updateProviderMutation.mutateAsync({
        provider,
        originalId,
        editorSave,
      });
    },
    [updateProviderMutation],
  );
  // 切换供应商
  const switchProvider = useCallback(
    async (
      provider: Provider,
      options?: {
        acknowledgedRouting?: boolean;
      },
    ) => {
      // 必须开启当前应用的 takeover（只看全局进程会漏判别的应用已接管的情况）。
      // 确认框 F 里选了「仍然直连切换」的不再重复提醒。
      const routingReady =
        isProxyTakeover === true || options?.acknowledgedRouting === true;
      const proxyRequiredReason = routingReady
        ? null
        : getRoutingReason(activeApp, provider, t);
      if (proxyRequiredReason) {
        logFrontendInfo(
          `[SWITCH] ${activeApp} 直连切到 ${provider.id}，提示需要路由：${proxyRequiredReason}`,
        );
        toast.warning(
          t("notifications.proxyRequiredForSwitch", {
            reason: proxyRequiredReason,
            defaultValue:
              "此供应商{{reason}}，需要代理服务才能正常使用，请先启动代理",
          }),
        );
      }
      // Codex official account cards can reuse the active native ChatGPT login
      // through local routing. Other apps' official providers remain blocked.
      const officialSupportsTakeover = supportsOfficialProxyTakeover(
        activeApp,
        provider,
      );
      if (
        isProxyTakeover &&
        provider.category === "official" &&
        !officialSupportsTakeover
      ) {
        logFrontendInfo(
          `[SWITCH] ${activeApp} 拒绝切到 ${provider.id}：路由模式下不能切到官方供应商`,
        );
        toast.error(
          t("notifications.officialBlockedByProxy", {
            defaultValue:
              "代理接管模式下不能切换到官方供应商，使用代理访问官方 API 可能导致账号被封禁",
          }),
          { duration: 6000 },
        );
        return;
      }
      try {
        const result = await switchProviderMutation.mutateAsync(provider.id);
        await syncClaudePlugin(provider);
        // Surface switch warnings by code — a generic "backfill failed"
        // message for an auth-cleanup warning would point the user at the
        // wrong problem entirely.
        if (result?.warnings?.length) {
          const coded = [
            {
              code: "codex_auth_cleanup_failed",
              message: t("notifications.codexAuthCleanupFailed", {
                defaultValue:
                  "切换成功，但未能删除 auth.json，官方登录凭据仍留在磁盘上；如需彻底移除请手动删除 Codex 配置目录中的 auth.json",
              }),
            },
            {
              code: "omo_variant_cleanup_failed",
              message: t("notifications.omoVariantCleanupFailed", {
                defaultValue:
                  "切换成功，但未能删除另一个 OMO 变体（OMO / OMO Slim）的配置文件，OpenCode 可能同时加载两者；请在 OpenCode 配置目录中手动删除它",
              }),
            },
          ];
          for (const { code, message } of coded) {
            if (result.warnings.some((warning) => warning.startsWith(code))) {
              toast.warning(message, { duration: 6000 });
            }
          }
          const hasOtherWarnings = result.warnings.some(
            (warning) => !coded.some(({ code }) => warning.startsWith(code)),
          );
          if (hasOtherWarnings) {
            toast.warning(
              t("notifications.backfillWarning", {
                defaultValue:
                  "切换成功，但旧供应商配置回填失败，您手动修改的配置可能未保存",
              }),
              { duration: 5000 },
            );
          }
        }
        // 若已弹过 proxyRequired 警告则不再弹 success
        if (!proxyRequiredReason) {
          let messageKey = "notifications.switchSuccess";
          let defaultMessage = "切换成功！";
          if (activeApp === "codex") {
            messageKey = "notifications.codexRestartRequired";
            defaultMessage = "切换成功，请重启客户端以生效";
          } else if (activeApp === "opencode") {
            messageKey = "notifications.addToConfigSuccess";
            defaultMessage = "已添加到配置";
          }
          const message = result?.route
            ? routeSwitchNotification(result.route, activeApp, provider.name, t)
            : t(messageKey, { defaultValue: defaultMessage });
          toast.success(message, {
            closeButton: true,
          });
        }
      } catch {
        // 错误提示由 mutation 处理
      }
    },
    [switchProviderMutation, syncClaudePlugin, activeApp, isProxyTakeover, t],
  );
  // 删除供应商
  const deleteProvider = useCallback(
    async (id: string) => {
      await deleteProviderMutation.mutateAsync(id);
    },
    [deleteProviderMutation],
  );
  // 保存用量脚本
  const saveUsageScript = useCallback(
    async (provider: Provider, script: UsageScript) => {
      try {
        const updatedProvider: Provider = {
          ...provider,
          meta: {
            ...provider.meta,
            usage_script: script,
          },
        };
        {
          const updated = await providersApi.update(updatedProvider, activeApp);
          if (!updated) {
            throw new Error(
              t("provider.usageSaveFailed", {
                defaultValue: "用量查询配置保存失败",
              }),
            );
          }
        }
        // Discard reads started before the save, including cold quota requests.
        await Promise.all(
          [
            ["providers", activeApp],
            usageKeys.script(provider.id, activeApp),
            ["subscription", "quota", activeApp],
          ].map((queryKey) =>
            invalidateQueriesAfterChange(queryClient, { queryKey }),
          ),
        );
        toast.success(
          t("provider.usageSaved", {
            defaultValue: "用量查询配置已保存",
          }),
          { closeButton: true },
        );
      } catch (error) {
        const detail =
          extractErrorMessage(error) ||
          t("provider.usageSaveFailed", {
            defaultValue: "用量查询配置保存失败",
          });
        toast.error(detail);
      }
    },
    [activeApp, queryClient, t],
  );
  // OpenCode：只写 opencode.json 顶层 model，small_model 不动
  const setOpenCodeDefaultModel = useCallback(
    async (provider: Provider, modelId?: string) => {
      const modelIds = getOpencodeDeclaredModels(
        provider.id,
        provider.settingsConfig,
      ).map((model) => model.id);
      if (modelIds.length === 0) {
        toast.error(
          t("notifications.opencodeNoModels", {
            defaultValue: "该供应商没有配置模型",
          }),
        );
        return;
      }
      const selected = modelId ?? modelIds[0];
      if (!modelIds.includes(selected)) {
        toast.error(
          t("notifications.opencodeModelNotFound", {
            defaultValue: "所选模型已不存在，请刷新后重试",
          }),
        );
        return;
      }
      try {
        await providersApi.setOpenCodeDefaultModel(
          `${provider.id}/${selected}`,
        );
        await invalidateQueriesAfterChange(queryClient, {
          queryKey: ["opencodeDefaultModel"],
        });
        toast.success(
          t("notifications.opencodeDefaultModelSet", {
            defaultValue: "已设为默认模型",
          }),
          { closeButton: true },
        );
      } catch (error) {
        toast.error(
          extractErrorMessage(error) ||
            t("notifications.opencodeDefaultModelSetFailed", {
              defaultValue: "设置默认模型失败",
            }),
        );
      }
    },
    [queryClient, t],
  );
  const setAsDefaultModel = setOpenCodeDefaultModel;
  return {
    addProvider,
    updateProvider,
    switchProvider,
    deleteProvider,
    saveUsageScript,
    setAsDefaultModel,
    isLoading:
      addProviderMutation.isPending ||
      updateProviderMutation.isPending ||
      deleteProviderMutation.isPending ||
      switchProviderMutation.isPending,
  };
}
