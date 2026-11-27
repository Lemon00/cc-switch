import { ConfirmDialog } from "@/components/ConfirmDialog";
import { CodexStaleClientsNotice } from "@/components/providers/CodexStaleClientsNotice";
import {
  blockedFromRouting,
  queueMoveSortUpdates,
} from "@/components/providers/presentation";
import { ProviderList } from "@/components/providers/ProviderList";
import { APP_DISPLAY_NAME } from "@/components/shell/AppGlyph";
import { badgeVariants } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { HoverTip } from "@/components/ui/hover-tip";
import { Notice } from "@/components/ui/notice";
import type { ModeAppId } from "@/config/appConfig";
import { isProxyAppId, isStackAppId } from "@/config/appConfig";
import { useModeActions } from "@/hooks/useModeActions";
import { settingsApi } from "@/lib/api";
import { providersApi } from "@/lib/api/providers";
import { proxyApi } from "@/lib/api/proxy";
import { useSettingsQuery } from "@/lib/query";
import {
  useAddToFailoverQueue,
  useAutoFailoverEnabled,
  useFailoverQueue,
  useRemoveFromFailoverQueue,
  useSetAutoFailoverEnabled,
} from "@/lib/query/failover";
import {
  proxyKeys,
  useAdoptCodexStackCatalog,
  useAppMode,
  useProxyStack,
  useProxyStatusQuery,
  useSetProxyStackMember,
} from "@/lib/query/proxy";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import type { Provider } from "@/types";
import type { AppMode, StartupAttachFailure } from "@/types/proxy";
import { extractErrorMessage } from "@/utils/errorUtils";
import { CODEX_OFFICIAL_PROVIDER_ID } from "@/utils/providerCapabilities";
import { routeSwitchNotification } from "@/utils/routeSwitchNotification";
import { getRoutingReason } from "@/utils/routingReason";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { ModeDialog, type ModeDialogState } from "./ModeDialog";
import { MODE_TONE, ModeTabs } from "./ModeTabs";
import { RouteSettingsSheet } from "./RouteSettingsSheet";
import { StackSettingsSheet } from "./StackSettingsSheet";
import {
  sameStackSubagent,
  stackSubagentSettings,
  stackSubagentValue,
  type StackSubagentValue,
} from "./StackSubagentFields";
type ListCallbacks = Pick<
  React.ComponentProps<typeof ProviderList>,
  | "onEdit"
  | "onDelete"
  | "onDuplicate"
  | "onConfigureUsage"
  | "onOpenWebsite"
  | "onOpenTerminal"
  | "onCreate"
  | "searchOpen"
  | "onSearchOpenChange"
>;
interface SwitchModePanelProps extends ListCallbacks {
  app: ModeAppId;
  statusSlot?: HTMLDivElement | null;
  providers: Record<string, Provider>;
  currentProviderId: string;
  isLoading: boolean;
  scrollRef?: RefObject<HTMLDivElement>;
  onSwitch: (
    provider: Provider,
    options?: {
      acknowledgedRouting?: boolean;
    },
  ) => unknown;
  onOpenRoutingSettings: () => void;
  startupFailure?: StartupAttachFailure;
  onDismissStartupFailure?: () => void;
  /** 托盘里点了直连下需要路由的那家：到这页后弹同一个「需要路由」对话框 */
  needsRouteRequest?: {
    providerId: string;
    nonce: number;
  };
  onNeedsRouteHandled?: () => void;
  /** 正在看的那格变了：新增 / 编辑供应商按它选表单布局。需要是稳定的回调 */
  onViewChange?: (app: ModeAppId, view: AppMode) => void;
}
/** Gemini CLI、Grok Build、Claude Desktop 没有聚合模式（Q6）：那一格隐藏，前两格位置不变。 */
const modesFor = (app: ModeAppId): AppMode[] =>
  isStackAppId(app) ? ["direct", "route", "stack"] : ["direct", "route"];
/**
 * 切换式应用的供应商页（v7 S1–S3）：模式行 → 通知槽 → 按查看的模式分区的列表。
 * tab 只负责查看；要生效必须点写明后果的按钮（确认框），回到直连也一样。
 */
export function SwitchModePanel({
  statusSlot,
  app,
  providers,
  currentProviderId,
  isLoading,
  scrollRef,
  onSwitch,
  onOpenRoutingSettings,
  startupFailure,
  onDismissStartupFailure,
  needsRouteRequest,
  onNeedsRouteHandled,
  onViewChange,
  ...listCallbacks
}: SwitchModePanelProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const appName = APP_DISPLAY_NAME[app];
  const modes = modesFor(app);
  const { data: modeView } = useAppMode(app);
  const active: AppMode =
    modeView && modes.includes(modeView.mode) ? modeView.mode : "direct";
  const directId = modeView?.directProviderId ?? null;
  const routeId = modeView?.routeProviderId ?? null;
  // 离开页面再回来、或生效的模式变了，一律落在生效的那格（「查看中」不持久化）
  const [view, setView] = useState<AppMode>(active);
  const [searchSlot, setSearchSlot] = useState<HTMLDivElement | null>(null);
  useEffect(() => {
    setView(active);
  }, [app, active]);
  useEffect(() => {
    onViewChange?.(app, view);
  }, [app, view, onViewChange]);
  const { data: proxyStatus } = useProxyStatusQuery();
  const serviceRunning = proxyStatus?.running ?? false;
  const { data: settings } = useSettingsQuery();
  // Claude Desktop 没有故障转移和路由设置（没有 proxy_config 行）
  const hasFailover = isProxyAppId(app);
  const {
    data: failoverOn = false,
    isSuccess: failoverEnabledReady,
    isError: failoverEnabledError,
  } = useAutoFailoverEnabled(app, hasFailover);
  const { data: queue } = useFailoverQueue(app, hasFailover);
  const setFailover = useSetAutoFailoverEnabled();
  const addToQueue = useAddToFailoverQueue();
  const removeFromQueue = useRemoveFromFailoverQueue();
  const { data: stack } = useProxyStack(app, isStackAppId(app));
  const setStackMember = useSetProxyStackMember();
  const adoptCatalog = useAdoptCodexStackCatalog();
  const modeActions = useModeActions(app);
  const [dialog, setDialog] = useState<ModeDialogState | null>(null);
  const [routeSettingsOpen, setRouteSettingsOpen] = useState(false);
  const [stackSettingsOpen, setStackSettingsOpen] = useState(false);
  const [confirmFailover, setConfirmFailover] = useState(false);
  const [confirmFailoverPending, setConfirmFailoverPending] = useState(false);
  // 关掉的是哪一份提示（后端的 staleRevision）：目录或登录又变了就是新的一份，重新显示。
  const [staleDismissed, setStaleDismissed] = useState<string | null>(null);
  useEffect(() => {
    setStaleDismissed(null);
  }, [app, active, directId, routeId, stack?.staleClients?.auth]);
  const staleRevision = stack?.staleRevision ?? "";
  // 供应商还没加载完时先等着，到了再弹
  useEffect(() => {
    if (!needsRouteRequest) return;
    const provider = providers[needsRouteRequest.providerId];
    if (!provider) return;
    setDialog({
      kind: "needsRoute",
      providerId: provider.id,
      reason: getRoutingReason(app, provider, t) ?? "",
    });
    onNeedsRouteHandled?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needsRouteRequest?.nonce, providers]);
  const providerList = useMemo(() => Object.values(providers), [providers]);
  const nameOf = (id: string | null) =>
    (id && providers[id]?.name) || t("mode.noProvider");
  const stackMembers = useMemo(() => {
    const members = new Map<string, number>();
    for (const member of stack?.members ?? []) {
      members.set(member.providerId, member.modelIds.length);
    }
    return members;
  }, [stack?.members]);
  const queueIds = useMemo(
    () => (queue ?? []).map((item) => item.providerId),
    [queue],
  );
  const exitAndUse = (provider: Provider) =>
    setDialog({ kind: "exit", useProviderId: provider.id });
  const modeBusy =
    modeActions.pending ||
    setFailover.isPending ||
    confirmFailoverPending ||
    addToQueue.isPending ||
    removeFromQueue.isPending ||
    setStackMember.isPending;
  const queueMove = async (provider: Provider, delta: -1 | 1) => {
    if (modeBusy) return;
    const updates = queueMoveSortUpdates(
      queue ?? [],
      providers,
      provider.id,
      delta,
    );
    if (!updates) return;
    const target = updates.findIndex((update) => update.id === provider.id);
    try {
      await providersApi.updateSortOrder(updates, app);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["failoverQueue", app] }),
        queryClient.invalidateQueries({ queryKey: ["providers", app] }),
      ]);
      toast.success(
        t("mode.queueMoved", { name: provider.name, position: target + 1 }),
      );
    } catch (error) {
      toast.error(extractErrorMessage(error) || t("provider.sortUpdateFailed"));
    }
  };
  const rememberDefault = async (provider: Provider) => {
    if (modeBusy) return;
    try {
      const result = await proxyApi.setProxyRoute(app, provider.id);
      await queryClient.invalidateQueries({ queryKey: ["providers", app] });
      toast.success(routeSwitchNotification(result, app, provider.name, t), {
        closeButton: true,
      });
    } catch (error) {
      toast.error(
        t("mode.toast.failed", {
          detail: extractErrorMessage(error) || t("common.unknown"),
        }),
      );
    }
  };
  const switchMode = {
    active,
    view,
    directId,
    routeId,
    failoverOn,
    queue: queueIds,
    stackMembers,
    serviceRunning,
    routingReason: (provider: Provider) =>
      getRoutingReason(app, provider, t) ?? "",
    actions: {
      switchDirect: (provider: Provider) => {
        if (modeBusy) return;
        void onSwitch(provider);
      },
      needsRouteDialog: (provider: Provider) =>
        setDialog({
          kind: "needsRoute",
          providerId: provider.id,
          reason: getRoutingReason(app, provider, t) ?? "",
        }),
      exitAndUse: (provider: Provider) => {
        if (modeBusy) return;
        exitAndUse(provider);
      },
      routeTo: (provider: Provider) => {
        if (modeBusy) return;
        void onSwitch(provider);
      },
      queueAdd: (provider: Provider) => {
        if (modeBusy) return;
        addToQueue.mutate({ appType: app, providerId: provider.id });
      },
      queueRemove: (provider: Provider) => {
        if (modeBusy) return;
        removeFromQueue.mutate({ appType: app, providerId: provider.id });
      },
      queueMove: (provider: Provider, delta: -1 | 1) =>
        void queueMove(provider, delta),
      stackAdd: (provider: Provider) => {
        if (modeBusy) return;
        setStackMember.mutate({
          appType: app,
          providerId: provider.id,
          enabled: true,
        });
      },
      stackRemove: (provider: Provider) => {
        if (modeBusy) return;
        setStackMember.mutate({
          appType: app,
          providerId: provider.id,
          enabled: false,
        });
      },
      // 聚合生效时换默认当场生效（同切换供应商）；没生效时只记下选择，切换时的确认框预选它
      stackSetDefault: (provider: Provider) => {
        if (modeBusy) return;
        if (active === "stack") {
          void onSwitch(provider);
        } else {
          void rememberDefault(provider);
        }
      },
    },
  };
  // 页头始终显示实际生效的连接，不跟随正在浏览的模式页签。
  const routeTarget = failoverOn ? t("mode.status.byQueue") : nameOf(routeId);
  const otherMembers = (stack?.members ?? []).filter(
    (member) => member.providerId !== routeId,
  );
  const otherModels = new Set(
    (stack?.members ?? []).flatMap((member) => member.modelIds),
  ).size;
  const status =
    active === "direct"
      ? { lead: t("mode.status.directLead"), value: nameOf(directId) }
      : active === "route"
        ? {
            lead: t("mode.status.routeLead"),
            value: routeTarget,
          }
        : {
            lead: t("mode.status.stackLead"),
            value: t("mode.status.stackValue", {
              name: nameOf(routeId),
              count: otherMembers.length,
              models: otherModels,
            }),
          };
  const handleFailoverChange = (enabled: boolean) => {
    // 未拿到真实状态或读失败时不切换，避免 placeholder / 未知态下误操作
    if (!failoverEnabledReady || failoverEnabledError || modeBusy) return;
    if (enabled && !settings?.failoverConfirmed) {
      setConfirmFailover(true);
      return;
    }
    setFailover.mutate({ appType: app, enabled });
  };
  const confirmFailoverOn = async () => {
    if (
      confirmFailoverPending ||
      modeActions.pending ||
      setFailover.isPending
    ) {
      return;
    }
    setConfirmFailoverPending(true);
    try {
      await settingsApi.save({
        failoverConfirmed: true,
      });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
      setConfirmFailover(false);
      setFailover.mutate({ appType: app, enabled: true });
    } catch (error) {
      console.error(
        "[SwitchModePanel] Failed to save failover confirmation",
        error,
      );
      toast.error(
        t("mode.failoverConfirmSaveFailed", {
          defaultValue: "未能保存确认状态，故障转移未开启",
        }),
        { closeButton: true },
      );
    } finally {
      setConfirmFailoverPending(false);
    }
  };
  const enterMode = async (
    target: Exclude<AppMode, "direct">,
    pick: string,
    subagent?: StackSubagentValue,
  ) => {
    if (modeBusy) return;
    // 框里改了子代理设置就先存（聚合还没生效时后端只存不写），进入聚合时随契约一起写进去
    if (
      subagent &&
      !sameStackSubagent(subagent, stackSubagentValue(settings))
    ) {
      await settingsApi.save(stackSubagentSettings(subagent));
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    }
    await modeActions.enter(target, pick, providers[pick]?.name);
  };
  const eligibleIds = providerList
    .filter((provider) => !blockedFromRouting(app, provider))
    .map((provider) => provider.id);
  const startService = async () => {
    try {
      await proxyApi.startProxyServer();
      await queryClient.invalidateQueries({ queryKey: proxyKeys.status });
    } catch (error) {
      toast.error(
        t("proxy.server.startFailed", {
          detail: extractErrorMessage(error) || t("common.unknown"),
        }),
      );
    }
  };
  const notices: React.ReactNode[] = [];
  if (active !== "direct" && proxyStatus !== undefined && !serviceRunning) {
    notices.push(
      <Notice
        key="broken"
        tone="warning"
        title={t("mode.notice.serviceDown", {
          app: appName,
          mode: t(`mode.names.${active}`),
        })}
        actions={
          <>
            <Button
              variant="neutral"
              size="compact"
              onClick={() => void startService()}
            >
              {t("mode.notice.startService")}
            </Button>
            <Button
              variant="neutral"
              size="compact"
              disabled={modeBusy}
              onClick={() => {
                if (modeBusy) return;
                setDialog({ kind: "exit" });
              }}
            >
              {t("mode.notice.backToDirect")}
            </Button>
          </>
        }
      />,
    );
  }
  if (startupFailure) {
    notices.push(
      <Notice
        key="startup"
        tone="warning"
        title={t("mode.notice.attachFailed", {
          app: appName,
          mode: t(`mode.names.${startupFailure.stack ? "stack" : "route"}`),
        })}
        onDismiss={onDismissStartupFailure}
        dismissLabel={t("common.close")}
        actions={
          <Button
            variant="neutral"
            size="compact"
            disabled={modeBusy}
            onClick={() => {
              if (modeBusy) return;
              void modeActions
                .enter(
                  startupFailure.stack ? "stack" : "route",
                  routeId,
                  nameOf(routeId),
                )
                .then(() => onDismissStartupFailure?.())
                .catch((error) =>
                  toast.error(
                    t("mode.toast.failed", {
                      detail: extractErrorMessage(error) || t("common.unknown"),
                    }),
                  ),
                );
            }}
          >
            {t("common.retry")}
          </Button>
        }
      >
        {startupFailure.error}
      </Notice>,
    );
  }
  if (view === "stack" && active === "stack" && stack?.notice) {
    notices.push(
      <Notice
        key="stackNotice"
        tone="warning"
        title={t(`provider.${stack.notice}`)}
        actions={
          stack.notice === "routeOwnsCatalog" ? (
            <Button
              variant="neutral"
              size="compact"
              disabled={adoptCatalog.isPending}
              onClick={() => adoptCatalog.mutate()}
            >
              {t("provider.adoptCatalog")}
            </Button>
          ) : undefined
        }
      >
        {stack.noticeReasons?.length
          ? stack.noticeReasons.map((reason) => (
              <div key={reason}>
                {t(`provider.officialModelsReason.${reason}`)}
              </div>
            ))
          : undefined}
      </Notice>,
    );
  }
  if (
    app === "codex" &&
    stack?.staleClients &&
    (stack.staleClients.daemon || stack.staleClients.others) &&
    staleDismissed !== staleRevision
  ) {
    notices.push(
      <CodexStaleClientsNotice
        key="stale"
        staleClients={stack.staleClients}
        onDismiss={() => setStaleDismissed(staleRevision)}
      />,
    );
  }
  if (view !== active) {
    const cta =
      view === "route"
        ? t("mode.activate.route")
        : view === "stack"
          ? t("mode.activate.stack")
          : t("mode.activate.direct");
    notices.push(
      <Notice
        key="activate"
        tone="neutral"
        title={t("mode.activate.viewing", {
          view: t(`mode.names.${view}`),
          app: appName,
          active: t(`mode.names.${active}`),
        })}
        actions={
          <Button
            variant={view}
            size="regular"
            disabled={modeBusy}
            onClick={() => {
              if (modeBusy) return;
              setDialog(
                view === "direct"
                  ? { kind: "exit" }
                  : { kind: "enter", target: view },
              );
            }}
          >
            {cta}
          </Button>
        }
      />,
    );
  }
  return (
    <>
      {statusSlot &&
        createPortal(
          <HoverTip content={`${status.lead} ${status.value}`}>
            <span
              tabIndex={0}
              aria-label={`${status.lead} ${status.value}`}
              className={cn(
                badgeVariants({ variant: "outline" }),
                "max-w-full gap-1 border-border bg-surface px-2.5 py-1 text-body font-normal",
              )}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "me-1 h-1.5 w-1.5 shrink-0 rounded-full",
                  (
                    active === "direct"
                      ? Boolean(directId && providers[directId])
                      : serviceRunning &&
                        modeView?.attached &&
                        Boolean(routeId && providers[routeId])
                  )
                    ? MODE_TONE[active].dot
                    : "bg-fg-3",
                )}
              />
              <span className="shrink-0 text-fg-2">
                {t(`mode.names.${active}`)}
              </span>
              <span aria-hidden="true" className="text-fg-3">
                ·
              </span>
              <span className="min-w-0 truncate font-medium text-fg-1">
                {active === "stack" ? nameOf(routeId) : status.value}
              </span>
            </span>
          </HoverTip>,
          statusSlot,
        )}
      <ModeTabs
        modes={modes}
        active={active}
        view={view}
        onView={setView}
        failover={
          hasFailover && view === "route" && active === "route"
            ? {
                enabled: failoverOn,
                disabled:
                  modeBusy || !failoverEnabledReady || failoverEnabledError,
                onChange: handleFailoverChange,
              }
            : undefined
        }
        onOpenRouteSettings={
          hasFailover && view === "route" && active === "route"
            ? () => setRouteSettingsOpen(true)
            : undefined
        }
        onOpenStackSettings={
          (app === "codex" || app === "claude") &&
          view === "stack" &&
          active === "stack"
            ? () => setStackSettingsOpen(true)
            : undefined
        }
        searchSlotRef={setSearchSlot}
      />
      <div
        ref={scrollRef}
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable overflow-x-hidden px-6 pb-12 pt-3"
      >
        {notices.length > 0 && (
          <div role="status" className="mb-3 flex flex-col gap-2">
            {notices}
          </div>
        )}
        <ProviderList
          {...listCallbacks}
          providers={providers}
          currentProviderId={currentProviderId}
          appId={app}
          isLoading={isLoading}
          onSwitch={(provider) => {
            if (modeBusy) return;
            void onSwitch(provider);
          }}
          switchMode={switchMode}
          searchSlot={searchSlot}
        />
      </div>

      <ModeDialog
        app={app}
        state={dialog}
        active={active}
        providers={providerList}
        directId={directId}
        eligibleIds={eligibleIds}
        defaultPick={routeId ?? directId}
        stackMembers={(stack?.members ?? []).map((member) => ({
          id: member.providerId,
          name: nameOf(member.providerId),
          models: member.memberModels,
        }))}
        stackSubagent={
          app === "codex" && settings
            ? {
                models: stack?.subagentModels ?? [],
                value: stackSubagentValue(settings),
              }
            : undefined
        }
        onClose={() => setDialog(null)}
        onEnter={enterMode}
        onExit={(useProviderId) =>
          modeActions.exitToDirect({
            directProviderId: directId,
            useProviderId,
            providerName: nameOf(useProviderId ?? directId),
          })
        }
        onSwitchDirect={(providerId) => {
          const provider = providers[providerId];
          if (provider) void onSwitch(provider, { acknowledgedRouting: true });
        }}
      />

      {isProxyAppId(app) && (
        <RouteSettingsSheet
          app={app}
          open={routeSettingsOpen}
          onOpenChange={setRouteSettingsOpen}
          onOpenSettings={onOpenRoutingSettings}
        />
      )}

      {(app === "codex" || app === "claude") && (
        <StackSettingsSheet
          app={app}
          open={stackSettingsOpen}
          onOpenChange={setStackSettingsOpen}
          models={stack?.subagentModels ?? []}
          // 官方做默认那家时，合并目录里有官方的 codex-auto-review，自动审批默认用它
          routeOfficial={
            !!routeId &&
            (routeId === CODEX_OFFICIAL_PROVIDER_ID ||
              providers[routeId]?.category === "official")
          }
        />
      )}

      <ConfirmDialog
        isOpen={confirmFailover}
        variant="info"
        title={t("confirm.failover.title")}
        message={t("confirm.failover.message")}
        confirmText={t("confirm.failover.confirm")}
        pending={confirmFailoverPending}
        onConfirm={() => void confirmFailoverOn()}
        onCancel={() => {
          if (confirmFailoverPending) return;
          setConfirmFailover(false);
        }}
      />
    </>
  );
}
