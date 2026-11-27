import { ConfirmDialog } from "@/components/ConfirmDialog";
import { DeepLinkImportHost } from "@/components/DeepLinkImportHost";
import { WhatsNewNotice } from "@/components/WhatsNewNotice";
import { EnvWarningBanner } from "@/components/env/EnvWarningBanner";
import { ProfileSwitcher } from "@/components/profiles/ProfileSwitcher";
import { EditProviderHost } from "@/components/providers/EditProviderHost";
import { OpenTerminalDialog } from "@/components/providers/OpenTerminalDialog";
import { ProviderList } from "@/components/providers/ProviderList";
import { useCreateEditProviderStore } from "@/components/providers/editProviderStore";
import { hasOpencodeDefinition } from "@/components/providers/forms/helpers/opencodeFormUtils";
import { SwitchModePanel } from "@/components/providers/mode/SwitchModePanel";
import { RefreshAllQuotaButton } from "@/components/settings/auth/RefreshAllQuotaButton";
import { APP_DISPLAY_NAME, AppGlyph } from "@/components/shell/AppGlyph";
import {
  AppPageHeader,
  WindowControlsContext,
} from "@/components/shell/AppPageHeader";
import { NewLayoutDialog } from "@/components/shell/NewLayoutDialog";
import { Sidebar } from "@/components/shell/Sidebar";
import { WindowControls } from "@/components/shell/WindowControls";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { HelpTip } from "@/components/ui/help-tip";
import { HoverTip } from "@/components/ui/hover-tip";
import { Notice } from "@/components/ui/notice";
import {
  DEFAULT_VISIBLE_APPS,
  isModeAppId,
  orderAppIds,
} from "@/config/appConfig";
import { useUpdate } from "@/contexts/UpdateContext";
import { useLastValidValue } from "@/hooks/useLastValidValue";
import { useProfileCacheBridge } from "@/hooks/useProfileCacheBridge";
import { useProviderActions } from "@/hooks/useProviderActions";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { useStartupFailures } from "@/hooks/useStartupFailures";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import {
  useTrayAppPageSeen,
  useTrayNavigation,
} from "@/hooks/useTrayNavigation";
import { useUsageCacheBridge } from "@/hooks/useUsageCacheBridge";
import {
  providersApi,
  settingsApi,
  type AppId,
  type ProviderSwitchEvent,
} from "@/lib/api";
import {
  checkAllEnvConflicts,
  checkEnvConflicts,
  flattenEnvConflicts,
  summarizeEnvCheckFailures,
} from "@/lib/api/env";
import type { ProviderEditorSave } from "@/lib/api/providers";
import {
  appPageBelongsTo,
  isAppPage,
  readStoredApp,
  readStoredView,
  sharedFeatureAppOf,
  storeApp,
  storeView,
  type GlobalPage,
  type SettingsSection,
  type View,
} from "@/lib/navigation";
import { isLinux, isMac, isWindows } from "@/lib/platform";
import { isNativeOpencodeConfig } from "@/lib/providerConfig";
import { useProvidersQuery, useSettingsQuery } from "@/lib/query";
import { invalidateQueriesAfterChange } from "@/lib/query/invalidation";
import {
  useDisableCurrentOmo,
  useDisableCurrentOmoSlim,
} from "@/lib/query/omo";
import { PROMPT_APP_IDS } from "@/lib/query/prompts";
import {
  deleteProviderAndRefresh,
  refreshProviderLiveState,
  settleProviderChange,
} from "@/lib/query/providerActions";
import { invalidateProviderCaches } from "@/lib/query/providerCache";
import { toast } from "@/lib/toast";
import {
  checkToolUpdatesInBackground,
  useToolUpdatesAvailable,
} from "@/lib/toolUpdates";
import { discardUnsavedChanges, hasUnsavedChanges } from "@/lib/unsavedChanges";
import type { Provider, VisibleApps } from "@/types";
import type { EnvConflict } from "@/types/env";
import type { AppMode } from "@/types/proxy";
import { KNOWN_APP_TYPES, type AppTypeFilter } from "@/types/usage";
import { deepClone } from "@/utils/deepClone";
import { isTextEditableTarget } from "@/utils/domUtils";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { KeyRound, MoreHorizontal, Plus, Search } from "lucide-react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
const FirstRunNoticeDialog = lazy(() =>
  import("@/components/FirstRunNoticeDialog").then((m) => ({
    default: m.FirstRunNoticeDialog,
  })),
);
// Keep the provider page and event listeners ready at startup. Editors, charts
// and secondary pages are loaded when the user first opens them.
const AddProviderDialog = lazy(() =>
  import("@/components/providers/AddProviderDialog").then((m) => ({
    default: m.AddProviderDialog,
  })),
);
const SettingsPage = lazy(() =>
  import("@/components/settings/SettingsPage").then((m) => ({
    default: m.SettingsPage,
  })),
);
const AuthCenterPanel = lazy(() =>
  import("@/components/settings/AuthCenterPanel").then((m) => ({
    default: m.AuthCenterPanel,
  })),
);
const AppsPage = lazy(() =>
  import("@/components/apps/AppsPage").then((m) => ({ default: m.AppsPage })),
);
const UsagePage = lazy(() =>
  import("@/components/usage/UsagePage").then((m) => ({
    default: m.UsagePage,
  })),
);
const SessionManagerPage = lazy(() =>
  import("@/components/sessions/SessionManagerPage").then((m) => ({
    default: m.SessionManagerPage,
  })),
);
const UsageScriptModal = lazy(() => import("@/components/UsageScriptModal"));
const UnifiedMcpPanel = lazy(() => import("@/components/mcp/UnifiedMcpPanel"));
const PromptPanel = lazy(() => import("@/components/prompts/PromptPanel"));
const UnifiedSkillsPanel = lazy(
  () => import("@/components/skills/UnifiedSkillsPanel"),
);
function PageLoading() {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      className="flex flex-1 items-center justify-center text-sm text-fg-2"
    >
      {t("common.loading")}
    </div>
  );
}
interface SyncStatusUpdatedPayload {
  source?: string;
  status?: string;
  error?: string;
}
function App() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { hasUpdate } = useUpdate();
  const [activeApp, setActiveApp] = useState<AppId>(readStoredApp);
  const sharedFeatureApp = sharedFeatureAppOf(activeApp);
  const [requestedView, setCurrentView] = useState<View>(readStoredView);
  // 在渲染前校验所属应用，避免恢复旧导航或隐藏当前应用时先挂载错误的原生配置页。
  const currentView =
    isAppPage(requestedView) && !appPageBelongsTo(requestedView, activeApp)
      ? "providers"
      : requestedView;
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>("general");
  const [appConfigScrollTarget, setAppConfigScrollTarget] = useState<AppId>();
  // 进设置前停留的页面：设置目录里的「← 返回」回到这里
  const settingsReturnViewRef = useRef<View>("providers");
  const [promptsApp, setPromptsApp] = useState<AppId>(() =>
    PROMPT_APP_IDS.includes(sharedFeatureApp) ? sharedFeatureApp : "claude",
  );
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [hasOpenedAdd, setHasOpenedAdd] = useState(false);
  useEffect(() => {
    if (isAddOpen) setHasOpenedAdd(true);
  }, [isAddOpen]);
  // 供应商搜索面板：页头按钮和 ⌘F 都能打开；换应用或离开供应商页就收起
  const [providerSearchOpen, setProviderSearchOpen] = useState(false);
  const [connectionStatusSlot, setConnectionStatusSlot] =
    useState<HTMLDivElement | null>(null);
  useEffect(() => {
    setProviderSearchOpen(false);
  }, [activeApp, currentView]);
  // 供应商页顶部正在看的那格（直连 / 路由 / 聚合），由 SwitchModePanel 报上来。打开新增、
  // 编辑时记下当时那格（新增记在 formModeView，编辑随供应商记进 editProviderStore），
  // 表单按它选布局：在聚合那格打开就是聚合的简化表单。
  const [providerModeView, setProviderModeView] = useState<{
    app: AppId;
    view: AppMode;
  } | null>(null);
  const [formModeView, setFormModeView] = useState<AppMode>();
  const handleProviderModeViewChange = useCallback(
    (app: AppId, view: AppMode) => setProviderModeView({ app, view }),
    [],
  );
  // 托盘里点了直连下需要路由的那家：打开应用页后弹「需要路由」对话框
  const [trayNeedsRoute, setTrayNeedsRoute] = useState<{
    app: AppId;
    providerId: string;
    nonce: number;
  } | null>(null);
  const [mcpManagementBusy, setMcpManagementBusy] = useState(false);
  const [skillsNavigationBusy, setSkillsNavigationBusy] = useState(false);
  const [promptNavigationBusy, setPromptNavigationBusy] = useState(false);
  useEffect(() => {
    storeView(currentView);
  }, [currentView]);
  const { data: settingsData } = useSettingsQuery();
  // Windows 一律去掉系统标题栏，用页头里的应用内窗口按钮；Linux 由设置决定
  const useAppWindowControls =
    isWindows() || (isLinux() && (settingsData?.useAppWindowControls ?? false));
  const visibleApps = useMemo<VisibleApps>(
    () => ({
      ...DEFAULT_VISIBLE_APPS,
      ...settingsData?.visibleApps,
    }),
    [settingsData?.visibleApps],
  );
  // 「启动时检查应用更新」（默认关）：打开了才在后台查一次，查到新版本侧栏「应用」上出现圆点
  const checkToolUpdatesOnStartup =
    settingsData?.checkToolUpdatesOnStartup ?? false;
  const toolUpdatesAvailable = useToolUpdatesAvailable();
  const startupToolCheckDoneRef = useRef(false);
  useEffect(() => {
    if (!checkToolUpdatesOnStartup || startupToolCheckDoneRef.current) return;
    startupToolCheckDoneRef.current = true;
    void checkToolUpdatesInBackground();
  }, [checkToolUpdatesOnStartup]);
  // 启动时没能接上路由 / 聚合、已退回直连的应用：在对应的应用页提示一次并给「重试」
  const [startupFailures, setStartupFailures] = useStartupFailures();
  // 将已校正的视图写回状态，后续换回原应用也不能复活旧页面。
  useEffect(() => {
    if (requestedView !== currentView) setCurrentView(currentView);
  }, [requestedView, currentView]);
  // 正在编辑的供应商不放 App 的 state：开关编辑页时不让整个 App 和列表重渲染
  const editProviderStore = useCreateEditProviderStore();
  const [usageProvider, setUsageProvider] = useState<Provider | null>(null);
  const [confirmAction, setConfirmAction] = useState<{
    provider: Provider;
    action: "remove" | "delete";
    /** 打开确认框时固化的应用，避免托盘/侧栏切换后误删其它应用的同名供应商 */
    appId: AppId;
  } | null>(null);
  const [confirmActionPending, setConfirmActionPending] = useState(false);
  const [terminalDialog, setTerminalDialog] = useState<{
    provider: Provider;
    appId: AppId;
  } | null>(null);
  const [envConflicts, setEnvConflicts] = useState<EnvConflict[]>([]);
  const [envCheckFailure, setEnvCheckFailure] = useState<{
    apps: string[];
    message: string;
  } | null>(null);
  const [showEnvBanner, setShowEnvBanner] = useState(false);
  const [showEnvCheckFailureBanner, setShowEnvCheckFailureBanner] =
    useState(false);
  const effectiveUsageProvider = useLastValidValue(usageProvider);
  const mainScrollRef = useRef<HTMLElement>(null);
  const providerScrollContainerRef = useRef<HTMLDivElement>(null);
  useUsageCacheBridge();
  useLayoutEffect(() => {
    if (currentView !== "providers") return;
    for (const container of [
      mainScrollRef.current,
      providerScrollContainerRef.current,
    ]) {
      if (container) {
        container.scrollTop = 0;
        container.scrollLeft = 0;
      }
    }
  }, [activeApp, currentView]);
  const { isRunning: isProxyRunning, takeoverStatus } = useProxyStatus();
  const modeAppId = isModeAppId(activeApp) ? activeApp : null;
  // 换了应用、新面板还没报上来时为 undefined：表单按应用实际生效的模式
  const currentModeView =
    providerModeView?.app === activeApp ? providerModeView.view : undefined;
  const openAddProvider = (modeView: AppMode | undefined) => {
    setFormModeView(modeView);
    setIsAddOpen(true);
  };
  const currentAppUsesProxy = modeAppId !== null;
  const isCurrentAppTakeoverActive = modeAppId
    ? takeoverStatus?.[modeAppId]
    : false;
  const {
    data,
    isLoading,
    isError: providersIsError,
    error: providersError,
    refetch,
    isFetching: providersIsFetching,
  } = useProvidersQuery(activeApp, {
    isProxyRunning: currentAppUsesProxy && isProxyRunning,
  });
  // 首次失败且无缓存时不要用 {} 伪装成「没有供应商」
  const providers = useMemo(() => data?.providers ?? {}, [data]);
  const currentProviderId = data?.currentProviderId ?? "";
  const providersLoadFailed = providersIsError && !data;
  const {
    addProvider,
    updateProvider,
    switchProvider,
    saveUsageScript,
    setAsDefaultModel,
  } = useProviderActions(
    activeApp,
    isProxyRunning && isCurrentAppTakeoverActive,
  );
  // Kimi「添加」只写进 config.toml、不改默认模型；「切换」走 switchProvider
  const disableOmoMutation = useDisableCurrentOmo();
  const handleDisableOmo = () => {
    disableOmoMutation.mutate(undefined, {
      onSuccess: () => {
        toast.success(t("omo.disabled", { defaultValue: "OMO 已停用" }));
      },
      onError: (error: Error) => {
        toast.error(
          t("omo.disableFailed", {
            defaultValue: "停用 OMO 失败: {{error}}",
            error: extractErrorMessage(error),
          }),
        );
      },
    });
  };
  const disableOmoSlimMutation = useDisableCurrentOmoSlim();
  const handleDisableOmoSlim = () => {
    disableOmoSlimMutation.mutate(undefined, {
      onSuccess: () => {
        toast.success(t("omo.disabled", { defaultValue: "OMO 已停用" }));
      },
      onError: (error: Error) => {
        toast.error(
          t("omo.disableFailed", {
            defaultValue: "停用 OMO 失败: {{error}}",
            error: extractErrorMessage(error),
          }),
        );
      },
    });
  };
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let active = true;
    const setupListener = async () => {
      try {
        const off = await providersApi.onSwitched(
          async (event: ProviderSwitchEvent) => {
            await settleProviderChange(queryClient, event.appType);
            await invalidateProviderCaches(
              queryClient,
              event.appType,
              "switch",
            );
          },
        );
        if (!active) {
          off();
          return;
        }
        unsubscribe = off;
      } catch (error) {
        console.error("[App] Failed to subscribe provider switch event", error);
      }
    };
    void setupListener();
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [queryClient]);
  useTauriEvent<{
    id?: string;
  }>("universal-provider-synced", async (payload) => {
    await Promise.all(
      (["claude", "codex"] as const).map((app) =>
        invalidateProviderCaches(
          queryClient,
          app,
          "update",
          payload?.id ? [`universal-${app}-${payload.id}`] : [],
        ),
      ),
    );
    try {
      await providersApi.updateTrayMenu();
    } catch (error) {
      console.error("[App] Failed to update tray menu", error);
    }
  });
  useProfileCacheBridge();
  useTauriEvent<SyncStatusUpdatedPayload | null | undefined>(
    "webdav-sync-status-updated",
    async (payload) => {
      const statusPayload = payload ?? {};
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
      if (statusPayload.source !== "auto" || statusPayload.status !== "error") {
        return;
      }
      toast.error(
        t("settings.webdavSync.autoSyncFailedToast", {
          error: statusPayload.error || t("common.unknown"),
        }),
      );
    },
  );
  useTauriEvent<SyncStatusUpdatedPayload | null | undefined>(
    "s3-sync-status-updated",
    async (payload) => {
      const statusPayload = payload ?? {};
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
      if (statusPayload.source !== "auto" || statusPayload.status !== "error") {
        return;
      }
      toast.error(
        t("settings.s3Sync.autoSyncFailedToast", {
          error: statusPayload.error || t("common.unknown"),
        }),
      );
    },
  );
  useTauriEvent<{
    appType: string;
    providerName: string;
  }>("proxy-official-warning", (payload) => {
    toast.warning(
      t("notifications.proxyOfficialWarning", {
        name: payload.providerName,
        defaultValue: `当前供应商 ${payload.providerName} 是官方供应商，建议切换到第三方供应商后再使用代理接管`,
      }),
      { duration: 8000 },
    );
  });
  useEffect(() => {
    // settingsData 未加载时跳过，避免用 fallback false 覆盖 Rust 侧已设好的装饰状态
    if (!settingsData) return;
    const syncWindowDecorations = async () => {
      try {
        await getCurrentWindow().setDecorations(!useAppWindowControls);
      } catch (error) {
        console.error("[App] Failed to update window decorations", error);
      }
    };
    void syncWindowDecorations();
  }, [useAppWindowControls, settingsData]);
  useEffect(() => {
    if (!isLinux() || !useAppWindowControls) return;
    const win = getCurrentWindow();
    let active = true;
    let revision = 0;
    let canResize = false;
    let nativeScale = 1;
    const unlisteners: (() => void)[] = [];
    let cursor:
      | {
          target: HTMLElement | SVGElement;
          value: string;
          priority: string;
          applied: string;
        }
      | undefined;
    const restoreCursor = () => {
      if (
        cursor &&
        cursor.target.style.cursor === cursor.applied &&
        cursor.target.style.getPropertyPriority("cursor") === "important"
      ) {
        cursor.target.style.setProperty(
          "cursor",
          cursor.value,
          cursor.priority,
        );
      }
      cursor = undefined;
    };
    const refresh = async () => {
      const current = ++revision;
      canResize = false;
      restoreCursor();
      try {
        const [maximized, fullscreen, resizable, scale] = await Promise.all([
          win.isMaximized(),
          win.isFullscreen(),
          win.isResizable(),
          win.scaleFactor(),
        ]);
        if (active && current === revision) {
          canResize = resizable && !maximized && !fullscreen;
          nativeScale = scale;
        }
      } catch (error) {
        console.error("[App] Failed to read window resize state", error);
      }
    };
    const edgeCursor = ({ clientX: x, clientY: y }: MouseEvent) => {
      if (
        !canResize ||
        x < 0 ||
        y < 0 ||
        x >= window.innerWidth ||
        y >= window.innerHeight
      ) {
        return "";
      }
      // 与 tauri-runtime-wry 的 GTK 边缘宽度（5 × scale，GDK 坐标）对齐，
      // 再从 GDK 坐标转换为 CSS 像素，兼容显示缩放和 WebView 缩放。
      const inset = (5 * nativeScale * nativeScale) / window.devicePixelRatio;
      const horizontal =
        x < inset ? -1 : x >= window.innerWidth - inset ? 1 : 0;
      const vertical = y < inset ? -1 : y >= window.innerHeight - inset ? 1 : 0;
      if (horizontal && vertical) {
        return horizontal === vertical ? "nwse-resize" : "nesw-resize";
      }
      return horizontal ? "ew-resize" : vertical ? "ns-resize" : "";
    };
    const onMouseMove = (event: MouseEvent) => {
      const value = edgeCursor(event);
      const target = event.target;
      if (
        !value ||
        !(target instanceof HTMLElement || target instanceof SVGElement)
      ) {
        restoreCursor();
        return;
      }
      if (cursor?.target !== target) {
        restoreCursor();
        cursor = {
          target,
          value: target.style.cursor,
          priority: target.style.getPropertyPriority("cursor"),
          applied: "",
        };
      }
      if (cursor.applied !== value) {
        cursor.applied = value;
        target.style.setProperty("cursor", value, "important");
      }
    };
    const onMouseDown = (event: MouseEvent) => {
      if (!(event.target instanceof Element)) return;
      const drag = event.target.getAttribute("data-tauri-drag-region");
      if (
        event.button === 0 &&
        drag !== null &&
        drag !== "false" &&
        edgeCursor(event)
      ) {
        // 保留 GTK 原生缩放，阻止 Tauri 的 document 冒泡监听再发起窗口移动。
        event.stopPropagation();
      }
    };
    document.addEventListener("mousemove", onMouseMove, true);
    document.addEventListener("mousedown", onMouseDown, true);
    document.documentElement.addEventListener("mouseleave", restoreCursor);
    window.addEventListener("blur", restoreCursor);
    for (const listener of [
      win.onResized(refresh),
      win.onScaleChanged(refresh),
    ]) {
      void listener
        .then((unlisten) => {
          if (active) unlisteners.push(unlisten);
          else unlisten();
        })
        .catch((error) =>
          console.error("[App] Failed to listen for window resize", error),
        );
    }
    void refresh();
    return () => {
      active = false;
      unlisteners.forEach((unlisten) => unlisten());
      document.removeEventListener("mousemove", onMouseMove, true);
      document.removeEventListener("mousedown", onMouseDown, true);
      document.documentElement.removeEventListener("mouseleave", restoreCursor);
      window.removeEventListener("blur", restoreCursor);
      restoreCursor();
    };
  }, [useAppWindowControls]);
  useEffect(() => {
    const checkEnvOnStartup = async () => {
      try {
        const allResults = await checkAllEnvConflicts();
        const failureSummary = summarizeEnvCheckFailures(allResults);
        if (failureSummary) {
          console.error(
            "[App] Partial environment conflict check failures:",
            allResults,
          );
          setEnvCheckFailure(failureSummary);
          const dismissed = sessionStorage.getItem(
            "env_check_failure_dismissed",
          );
          if (!dismissed) setShowEnvCheckFailureBanner(true);
        } else {
          setEnvCheckFailure(null);
          setShowEnvCheckFailureBanner(false);
        }
        const flatConflicts = flattenEnvConflicts(allResults);
        setEnvConflicts(flatConflicts);
        if (flatConflicts.length > 0) {
          const dismissed = sessionStorage.getItem("env_banner_dismissed");
          if (!dismissed) {
            setShowEnvBanner(true);
          }
        }
      } catch (error) {
        console.error(
          "[App] Failed to check environment conflicts on startup:",
          error,
        );
        setEnvCheckFailure({
          apps: [],
          message: extractErrorMessage(error) || String(error),
        });
        setShowEnvCheckFailureBanner(true);
      }
    };
    checkEnvOnStartup();
  }, []);
  useEffect(() => {
    const checkMigration = async () => {
      try {
        const migrated = await invoke<boolean>("get_migration_result");
        if (migrated) {
          toast.success(
            t("migration.success", { defaultValue: "配置迁移成功" }),
            { closeButton: true },
          );
        }
      } catch (error) {
        console.error("[App] Failed to check migration result:", error);
      }
    };
    checkMigration();
  }, [t]);
  useEffect(() => {
    const checkSkillsMigration = async () => {
      try {
        const result = await invoke<{
          count: number;
          error?: string;
        } | null>("get_skills_migration_result");
        if (result?.error) {
          toast.error(t("migration.skillsFailed"), {
            description: t("migration.skillsFailedDescription"),
            closeButton: true,
          });
          console.error("[App] Skills SSOT migration failed:", result.error);
          return;
        }
        if (result && result.count > 0) {
          toast.success(t("migration.skillsSuccess", { count: result.count }), {
            closeButton: true,
          });
          await queryClient.invalidateQueries({ queryKey: ["skills"] });
        }
      } catch (error) {
        console.error("[App] Failed to check skills migration result:", error);
      }
    };
    checkSkillsMigration();
  }, [t, queryClient]);
  useEffect(() => {
    const checkEnvOnSwitch = async () => {
      if (activeApp === "dsh" || activeApp === "agy") return;
      try {
        const conflicts = await checkEnvConflicts(activeApp);
        // 该应用检查成功：从失败汇总里摘掉它；整批失败（apps 为空）也一并清掉
        setEnvCheckFailure((prev) => {
          if (!prev) return null;
          if (prev.apps.length === 0) return null;
          if (!prev.apps.includes(activeApp)) return prev;
          const apps = prev.apps.filter((app) => app !== activeApp);
          return apps.length === 0 ? null : { apps, message: prev.message };
        });
        if (conflicts.length > 0) {
          setEnvConflicts((prev) => {
            const existingKeys = new Set(
              prev.map((c) => `${c.varName}:${c.sourcePath}`),
            );
            const newConflicts = conflicts.filter(
              (c) => !existingKeys.has(`${c.varName}:${c.sourcePath}`),
            );
            return [...prev, ...newConflicts];
          });
          const dismissed = sessionStorage.getItem("env_banner_dismissed");
          if (!dismissed) {
            setShowEnvBanner(true);
          }
        }
      } catch (error) {
        console.error(
          "[App] Failed to check environment conflicts on app switch:",
          error,
        );
        const detail = extractErrorMessage(error) || String(error);
        setEnvCheckFailure((prev) => {
          const apps = prev?.apps.includes(activeApp)
            ? prev.apps
            : [...(prev?.apps ?? []), activeApp];
          return {
            apps,
            message: prev?.message
              ? `${prev.message}\n${activeApp}: ${detail}`
              : `${activeApp}: ${detail}`,
          };
        });
        const dismissed = sessionStorage.getItem("env_check_failure_dismissed");
        if (!dismissed) setShowEnvCheckFailureBanner(true);
      }
    };
    checkEnvOnSwitch();
  }, [activeApp]);
  const currentViewRef = useRef(currentView);
  const managementBusy =
    mcpManagementBusy || skillsNavigationBusy || promptNavigationBusy;
  const managementBusyRef = useRef(false);
  managementBusyRef.current = managementBusy;
  // 设置响应可能晚于用户打开表单。隐藏侧栏项不能让已有草稿换成别的 appId。
  useEffect(() => {
    if (
      !settingsData ||
      visibleApps[activeApp] ||
      managementBusy ||
      confirmActionPending ||
      isAddOpen ||
      usageProvider ||
      editProviderStore.getSnapshot() ||
      hasUnsavedChanges()
    )
      return;
    const nextApp = orderAppIds(settingsData.appOrder).find(
      (app) => visibleApps[app],
    );
    if (!nextApp) return;
    setConfirmAction(null);
    setTerminalDialog(null);
    setActiveApp(nextApp);
    storeApp(nextApp);
  }, [
    settingsData,
    visibleApps,
    activeApp,
    managementBusy,
    confirmActionPending,
    isAddOpen,
    usageProvider,
    editProviderStore,
  ]);
  useEffect(() => {
    currentViewRef.current = currentView;
  }, [currentView]);
  // ─── 导航 ───────────────────────────────────────────────────────────────
  // MCP / Skills / 提示词在进行中的操作会锁住导航（卸载面板会丢掉进行中的状态）
  // 添加 / 编辑 / 用量脚本面板只盖住内容区，侧栏还能点。离开当前页就把它们关掉：
  // 面板的 appId 跟着 activeApp 走，开着切应用会把 A 应用的供应商保存进 B 应用
  const closeProviderPanels = () => {
    setIsAddOpen(false);
    editProviderStore.close();
    setUsageProvider(null);
  };
  // 整页编辑器里有未保存的修改：先确认，确认放弃后再执行这次导航
  const [pendingLeave, setPendingLeave] = useState<(() => void) | null>(null);
  const confirmLeave = (proceed: () => void) => {
    if (!hasUnsavedChanges()) return false;
    setPendingLeave(() => proceed);
    return true;
  };
  const openSettings = (
    section: SettingsSection = "general",
    appConfigTarget?: AppId,
  ) => {
    if (managementBusyRef.current) return;
    if (confirmLeave(() => openSettings(section, appConfigTarget))) return;
    closeProviderPanels();
    if (currentViewRef.current !== "settings") {
      settingsReturnViewRef.current = currentViewRef.current;
    }
    setSettingsSection(section);
    setAppConfigScrollTarget(appConfigTarget);
    setCurrentView("settings");
  };
  const exitSettings = () => {
    if (confirmLeave(exitSettings)) return;
    setCurrentView(settingsReturnViewRef.current);
  };
  const selectApp = (app: AppId): boolean => {
    if (managementBusyRef.current) return false;
    if (confirmLeave(() => selectApp(app))) return false;
    closeProviderPanels();
    // 切应用时关掉绑定旧 activeApp 的对话框；进行中的删除/移除不能清 pending，否则会破防重入
    if (!confirmActionPending) {
      setConfirmAction(null);
    }
    setTerminalDialog(null);
    setActiveApp(app);
    storeApp(app);
    setCurrentView("providers");
    return true;
  };
  /** 应用内页签（OpenClaw/Hermes）统一走未保存保护，不得直接 setCurrentView */
  const changeAppView = (view: View) => {
    if (currentViewRef.current === view) return;
    if (confirmLeave(() => changeAppView(view))) return;
    closeProviderPanels();
    setCurrentView(view);
  };
  const openPage = (page: GlobalPage | "settings") => {
    if (page === "settings") {
      openSettings("general");
      return;
    }
    if (managementBusyRef.current) return;
    // 再点当前页不会离开：不弹确认，也不能清掉编辑页的未保存登记
    if (currentViewRef.current === page) return;
    if (confirmLeave(() => openPage(page))) return;
    closeProviderPanels();
    if (page === "prompts" && currentViewRef.current !== "prompts") {
      // 提示词页默认选中侧栏里最后选的那个应用
      setPromptsApp(
        PROMPT_APP_IDS.includes(sharedFeatureApp) ? sharedFeatureApp : "claude",
      );
    }
    setCurrentView(page);
  };
  useTrayAppPageSeen(currentView === "providers" ? activeApp : null);
  useTrayNavigation((navigation) => {
    const navigate = () => {
      if (managementBusyRef.current) return;
      if (navigation.section) {
        openSettings(navigation.section);
        return;
      }
      if (!navigation.app) return;
      if (!selectApp(navigation.app)) return;
      // 托盘先换应用：新应用那格还没报上来，按它实际生效的模式
      if (navigation.intent === "add") openAddProvider(undefined);
      if (navigation.intent === "needsRoute" && navigation.providerId) {
        setTrayNeedsRoute({
          app: navigation.app,
          providerId: navigation.providerId,
          nonce: Date.now(),
        });
      }
    };
    if (!managementBusyRef.current && !confirmLeave(navigate)) navigate();
  });
  // 侧栏、⌘K 进用量统计看全部应用；只有应用页 ⋯ 进来时带应用筛选
  const openPageFromNav = (page: GlobalPage | "settings") => {
    // 「设置」上的绿点说的是 CC Switch 有新版本，点进去直接到「关于」里的更新按钮
    if (page === "settings" && hasUpdate) {
      openSettings("about");
      return;
    }
    if (page === "usage") setUsageAppFilter("all");
    openPage(page);
  };
  const openSettingsRef = useRef(openSettings);
  openSettingsRef.current = openSettings;
  const [usageAppFilter, setUsageAppFilter] = useState<AppTypeFilter>("all");
  const exitSettingsRef = useRef(exitSettings);
  exitSettingsRef.current = exitSettings;
  const leaveToSkills = () => {
    if (confirmLeave(() => setCurrentView("skills"))) return;
    setCurrentView("skills");
  };
  const leaveToSkillsRef = useRef(leaveToSkills);
  leaveToSkillsRef.current = leaveToSkills;
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key === ",") {
        event.preventDefault();
        openSettingsRef.current("general");
        return;
      }
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (document.body.style.overflow === "hidden") return;
      if (managementBusyRef.current) return;
      if (isTextEditableTarget(event.target)) return;
      const view = currentViewRef.current;
      if (view === "skillsDiscovery") {
        event.preventDefault();
        leaveToSkillsRef.current();
      } else if (view === "settings") {
        event.preventDefault();
        exitSettingsRef.current();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);
  const handleOpenWebsite = async (url: string) => {
    try {
      await settingsApi.openExternal(url);
    } catch (error) {
      const detail =
        extractErrorMessage(error) ||
        t("notifications.openLinkFailed", {
          defaultValue: "链接打开失败",
        });
      toast.error(detail);
    }
  };
  const handleEditProvider = async ({
    provider,
    originalId,
    editorSave,
  }: {
    provider: Provider;
    originalId?: string;
    editorSave?: ProviderEditorSave;
  }) => {
    await updateProvider(provider, originalId, editorSave);
    editProviderStore.close();
  };
  // 累加式应用的「已添加」和「默认」都现读客户端文件，移除前后重读一遍
  const refreshAdditiveLiveState = (app: AppId) =>
    refreshProviderLiveState(queryClient, app);
  const handleConfirmAction = async () => {
    if (!confirmAction || confirmActionPending) return;
    const { provider, action, appId } = confirmAction;
    setConfirmActionPending(true);
    try {
      if (action === "remove") {
        // Remove from live config only (for additive mode apps like OpenCode/OpenClaw)
        // Does NOT delete from database - provider remains in the list
        try {
          const removed = await providersApi.removeFromLiveConfig(
            provider.id,
            appId,
          );
          if (!removed) {
            throw new Error(t("common.unknown"));
          }
        } catch (error) {
          const detail = extractErrorMessage(error);
          const description = detail;
          // 默认模型可能在客户端里改过、页面还是旧的：失败了也重读，让「默认」和移除按钮跟上
          void refreshAdditiveLiveState(appId).catch(() => undefined);
          toast.error(t("notifications.removeFromConfigFailed"), {
            description: description || t("common.unknown"),
            closeButton: true,
          });
          return;
        }
        try {
          await refreshAdditiveLiveState(appId);
        } catch (error) {
          toast.error(extractErrorMessage(error) || t("common.unknown"), {
            closeButton: true,
          });
          return;
        }
        toast.success(
          t("notifications.removeFromConfigSuccess", {
            defaultValue: "已从配置移除",
          }),
          { closeButton: true },
        );
      } else {
        try {
          // 使用打开对话框时固化的 appId，不用当前 activeApp / 绑定它的 deleteProvider
          const deleted = await deleteProviderAndRefresh(
            queryClient,
            appId,
            provider.id,
          );
          if (!deleted) {
            throw new Error(
              t("notifications.deleteFailed", {
                defaultValue: "删除供应商失败: {{error}}",
                error: t("common.unknown"),
              }),
            );
          }
          toast.success(
            t("notifications.deleteSuccess", {
              defaultValue: "供应商已删除",
            }),
            { closeButton: true },
          );
        } catch (error) {
          const rawDetail = extractErrorMessage(error);
          const detail = rawDetail || t("common.unknown");
          toast.error(
            t("notifications.deleteFailed", {
              defaultValue: "删除供应商失败: {{error}}",
              error: detail,
            }),
            { closeButton: true },
          );
        }
      }
    } finally {
      setConfirmActionPending(false);
      setConfirmAction(null);
    }
  };
  const generateUniqueProviderCopyKey = (
    originalKey: string,
    existingKeys: string[],
  ): string => {
    const baseKey = `${originalKey}-copy`;
    if (!existingKeys.includes(baseKey)) {
      return baseKey;
    }
    let counter = 2;
    while (existingKeys.includes(`${baseKey}-${counter}`)) {
      counter++;
    }
    return `${baseKey}-${counter}`;
  };
  const handleDuplicateProvider = async (provider: Provider) => {
    if (
      activeApp === "opencode" &&
      provider.category !== "omo" &&
      provider.category !== "omo-slim"
    ) {
      const isNative = isNativeOpencodeConfig(
        JSON.stringify(provider.settingsConfig),
        provider.meta?.opencodeConfigFormat,
      );
      if (
        !hasOpencodeDefinition(
          provider.settingsConfig,
          isNative ? "package" : "npm",
        )
      ) {
        toast.error(
          t(
            isNative
              ? "opencode.duplicateRequiresNativeDefinition"
              : "opencode.duplicateRequiresDefinition",
          ),
        );
        return;
      }
    }
    const newSortIndex =
      provider.sortIndex !== undefined ? provider.sortIndex + 1 : undefined;
    const duplicatedProvider: Omit<Provider, "id" | "createdAt"> & {
      providerKey?: string;
      addToLive?: boolean;
    } = {
      name: `${provider.name} copy`,
      settingsConfig: deepClone(provider.settingsConfig),
      websiteUrl: provider.websiteUrl,
      notes: provider.notes,
      category: provider.category,
      sortIndex: newSortIndex, // 复制原 sortIndex + 1
      meta: provider.meta ? deepClone(provider.meta) : undefined,
      icon: provider.icon,
      iconColor: provider.iconColor,
    };
    if (activeApp === "opencode") {
      let liveProviderIds: string[] = [];
      try {
        liveProviderIds = await providersApi.getOpenCodeLiveProviderIds();
      } catch (error) {
        console.error(
          "[App] Failed to load live provider IDs for duplication",
          error,
        );
        const errorMessage = extractErrorMessage(error);
        toast.error(
          t("provider.duplicateLiveIdsLoadFailed", {
            defaultValue: "读取配置中的供应商标识失败，请先修复配置后再试",
          }) + (errorMessage ? `: ${errorMessage}` : ""),
        );
        return;
      }
      const existingKeys = Array.from(
        new Set([...Object.keys(providers), ...liveProviderIds]),
      );
      duplicatedProvider.providerKey = generateUniqueProviderCopyKey(
        provider.id,
        existingKeys,
      );
      duplicatedProvider.addToLive = false;
    } else if (activeApp === "dsh") {
      // The native catalog already includes live custom nodes; the backend
      // rejects keys owned by the client.
      duplicatedProvider.providerKey = generateUniqueProviderCopyKey(
        provider.id,
        Object.keys(providers),
      );
    }
    if (provider.sortIndex !== undefined) {
      const updates = Object.values(providers)
        .filter(
          (p) =>
            p.sortIndex !== undefined &&
            p.sortIndex >= newSortIndex! &&
            p.id !== provider.id,
        )
        .map((p) => ({
          id: p.id,
          sortIndex: p.sortIndex! + 1,
        }));
      if (updates.length > 0) {
        try {
          await providersApi.updateSortOrder(updates, activeApp);
        } catch (error) {
          console.error("[App] Failed to update sort order", error);
          toast.error(
            t("provider.sortUpdateFailed", {
              defaultValue: "排序更新失败",
            }),
          );
          return; // 如果排序更新失败，不继续添加
        }
      }
    }
    await addProvider(duplicatedProvider);
  };
  const confirmActionMessage = useMemo(() => {
    if (!confirmAction) return "";
    const message =
      confirmAction.action === "remove"
        ? t("confirm.removeProviderMessage", {
            name: confirmAction.provider.name,
          })
        : t("confirm.deleteProviderMessage", {
            name: confirmAction.provider.name,
          });
    return message;
  }, [confirmAction, t]);
  const handleProviderRefresh = async () => {
    try {
      await refetch();
    } catch (error) {
      toast.error(extractErrorMessage(error) || t("common.unknown"), {
        closeButton: true,
      });
    }
  };
  const handleImportSuccess = async () => {
    try {
      // SQL 恢复会替换 providers / skills / MCP / prompts / settings 等；
      // 不能只刷 providers，否则 staleTime: Infinity 的查询会继续展示导入前数据。
      await invalidateQueriesAfterChange(queryClient, { refetchType: "all" });
    } catch (error) {
      console.error("[App] Failed to refresh queries after import", error);
      await handleProviderRefresh();
    }
    try {
      await providersApi.updateTrayMenu();
    } catch (error) {
      console.error("[App] Failed to refresh tray menu", error);
    }
  };
  const handleHideActiveApp = async () => {
    const visibleCount = Object.values(visibleApps).filter(Boolean).length;
    if (visibleCount <= 1) return;
    try {
      await settingsApi.save({
        visibleApps: { ...visibleApps, [activeApp]: false },
      });
      await queryClient.invalidateQueries({ queryKey: ["settings"] });
    } catch (error) {
      toast.error(
        t("settings.saveFailedGeneric", { defaultValue: "保存失败，请重试" }),
        { description: extractErrorMessage(error) || undefined },
      );
    }
  };
  // ─── 应用页 ─────────────────────────────────────────────────────────────
  // 应用页 ⋯ →「查看此应用的用量」：带上这个应用的筛选（Claude Desktop 的流量并在 Claude 里）
  const usageAppOf = (app: AppId): AppTypeFilter | null => {
    const type = sharedFeatureAppOf(app);
    return (KNOWN_APP_TYPES as ReadonlyArray<string>).includes(type)
      ? (type as AppTypeFilter)
      : null;
  };
  const activeUsageApp = usageAppOf(activeApp);
  const appMenu = (
    <DropdownMenu>
      <HoverTip content={t("common.more")}>
        <DropdownMenuTrigger asChild>
          <Button
            variant="quiet"
            size="icon-compact"
            className="h-8 w-8"
            aria-label={t("appPage.moreActions", {
              name: APP_DISPLAY_NAME[activeApp],
            })}
          >
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
      </HoverTip>
      <DropdownMenuContent align="end" className="min-w-[180px]">
        <DropdownMenuItem onSelect={() => openPage("sessions")}>
          {t("appPage.viewSessions")}
        </DropdownMenuItem>
        {activeUsageApp && (
          <DropdownMenuItem
            onSelect={() => {
              setUsageAppFilter(activeUsageApp);
              openPage("usage");
            }}
          >
            {t("appPage.viewUsage")}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onSelect={() => openSettings("appConfig", activeApp)}>
          {t("appPage.configDirectory")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openPage("apps")}>
          {t("appPage.installAndUpgrade")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={Object.values(visibleApps).filter(Boolean).length <= 1}
          onSelect={() => void handleHideActiveApp()}
        >
          {t("appPage.hideFromSidebar")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
  const appPageName = undefined;
  const renderAppPageHeader = () => (
    <AppPageHeader
      variant="app"
      icon={<AppGlyph app={activeApp} size={20} badgeClassName="bg-app" />}
      title={APP_DISPLAY_NAME[activeApp]}
      truncateTitle={currentView === "providers" && Boolean(modeAppId)}
      titleExtra={
        currentView === "providers" && modeAppId ? (
          <div
            ref={setConnectionStatusSlot}
            className="ms-2 flex min-w-0 max-w-[280px] items-center"
          />
        ) : undefined
      }
      subtitle={
        currentView === "providers" && modeAppId ? undefined : appPageName
      }
      actions={
        <>
          {currentView === "providers" &&
            activeApp !== "dsh" &&
            (settingsData?.showProfileSwitcher ?? false) && (
              <ProfileSwitcher activeApp={activeApp} />
            )}
          {null}
          {currentView === "providers" &&
            (settingsData?.showProviderSearch ?? true) && (
              <HoverTip
                content={t("provider.searchButtonTip", {
                  shortcut: isMac() ? "⌘F" : "Ctrl+F",
                })}
              >
                <Button
                  variant="quiet"
                  size="icon-compact"
                  className="h-8 w-8"
                  aria-label={t("provider.searchAriaLabel")}
                  aria-pressed={providerSearchOpen}
                  onClick={() => setProviderSearchOpen((open) => !open)}
                >
                  <Search className="h-4 w-4" />
                </Button>
              </HoverTip>
            )}
          {currentView === "providers" && (
            <Button
              variant="solid"
              size="regular"
              onClick={() => openAddProvider(currentModeView)}
            >
              <Plus className="h-4 w-4" />
              {t("provider.addProvider")}
            </Button>
          )}
          {null}
          {appMenu}
        </>
      }
    />
  );
  // OpenClaw / Hermes 没有模式 tab，那一行放它们自己的页面导航。
  // 换整块内容的是页面导航，用下划线页签；分段控件只留给模式和页内筛选。
  const renderAppSegments = () => {
    return null;
  };
  const listCallbacks = {
    onEdit: (provider: Provider) =>
      editProviderStore.open(provider, currentModeView),
    onDelete: (provider: Provider) =>
      setConfirmAction({ provider, action: "delete", appId: activeApp }),
    onDuplicate: handleDuplicateProvider,
    onConfigureUsage: setUsageProvider,
    onOpenWebsite: handleOpenWebsite,
    onOpenTerminal:
      activeApp === "claude" || activeApp === "codex" || activeApp === "agy"
        ? (provider: Provider) =>
            setTerminalDialog({ provider, appId: activeApp })
        : undefined,
    onCreate: () => openAddProvider(currentModeView),
    searchOpen: providerSearchOpen,
    onSearchOpenChange: setProviderSearchOpen,
  };
  const providersErrorNotice =
    providersIsError && !isLoading ? (
      <Notice
        tone="danger"
        title={
          providersLoadFailed
            ? t("notifications.providersLoadFailed", {
                defaultValue: "无法加载供应商列表",
              })
            : t("notifications.providersRefreshFailed", {
                defaultValue: "供应商列表刷新失败，以下可能不是最新数据",
              })
        }
        actions={
          <Button
            variant="outline"
            size="sm"
            disabled={providersIsFetching}
            onClick={() => void handleProviderRefresh()}
          >
            {t("common.retry")}
          </Button>
        }
      >
        {extractErrorMessage(providersError) || t("common.unknown")}
      </Notice>
    ) : null;
  const renderProviderList = () => {
    if (modeAppId) {
      const startupFailure = startupFailures.find(
        (failure) => failure.appType === modeAppId,
      );
      return (
        <div className="flex min-h-0 flex-1 flex-col">
          {providersErrorNotice ? (
            <div className="shrink-0 px-6 pt-3">{providersErrorNotice}</div>
          ) : null}
          {providersLoadFailed ? null : (
            <SwitchModePanel
              key={modeAppId}
              app={modeAppId}
              statusSlot={connectionStatusSlot}
              providers={providers}
              currentProviderId={currentProviderId}
              isLoading={isLoading}
              scrollRef={providerScrollContainerRef}
              onSwitch={switchProvider}
              onOpenRoutingSettings={() => openSettings("routing")}
              needsRouteRequest={
                trayNeedsRoute?.app === modeAppId ? trayNeedsRoute : undefined
              }
              onNeedsRouteHandled={() => setTrayNeedsRoute(null)}
              onViewChange={handleProviderModeViewChange}
              startupFailure={startupFailure}
              onDismissStartupFailure={() =>
                setStartupFailures((list) =>
                  list.filter((failure) => failure.appType !== modeAppId),
                )
              }
              {...listCallbacks}
            />
          )}
        </div>
      );
    }
    return (
      <div
        ref={providerScrollContainerRef}
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable overflow-x-hidden px-6 pb-12 pt-4"
      >
        <div className="space-y-4">
          {providersErrorNotice}
          {providersLoadFailed ? null : (
            <ProviderList
              {...listCallbacks}
              providers={providers}
              currentProviderId={currentProviderId}
              appId={activeApp}
              isLoading={isLoading}
              onSwitch={(provider) => void switchProvider(provider)}
              onRemoveFromConfig={
                activeApp === "opencode" || activeApp === "dsh"
                  ? (provider) =>
                      setConfirmAction({
                        provider,
                        action: "remove",
                        appId: activeApp,
                      })
                  : undefined
              }
              onDisableOmo={
                activeApp === "opencode" ? handleDisableOmo : undefined
              }
              onDisableOmoSlim={
                activeApp === "opencode" ? handleDisableOmoSlim : undefined
              }
              onSetAsDefault={
                activeApp === "opencode"
                  ? (provider, modelId) =>
                      void setAsDefaultModel(provider, modelId)
                  : undefined
              }
            />
          )}
        </div>
      </div>
    );
  };
  const renderAppPage = () => {
    const body = (() => {
      switch (currentView) {
        default:
          return renderProviderList();
      }
    })();
    return (
      <>
        {renderAppPageHeader()}
        {renderAppSegments()}
        {null}
        {body}
      </>
    );
  };
  // ─── 全局页 ─────────────────────────────────────────────────────────────
  const renderGlobalPage = () => {
    switch (currentView) {
      case "usage":
        return (
          <UsagePage
            initialAppType={usageAppFilter}
            onOpenRoutingSettings={() => openSettings("routing")}
          />
        );
      case "auth":
        return (
          <>
            <AppPageHeader
              icon={<KeyRound className="h-5 w-5" strokeWidth={1.5} />}
              title={t("nav.auth")}
              titleExtra={
                <>
                  <HelpTip title={t("authCenter.helpTitle")}>
                    {t("settings.authCenter.description")}
                  </HelpTip>
                  <Badge
                    variant="outline"
                    className="ms-1 h-5 px-1.5 text-badge font-medium"
                  >
                    {t("settings.authCenter.beta", { defaultValue: "Beta" })}
                  </Badge>
                </>
              }
              actions={<RefreshAllQuotaButton />}
            />
            <div
              id="main-content"
              className="min-h-0 flex-1 overflow-y-auto scroll-stable"
            >
              <div className="px-6 pb-10 pt-4">
                <AuthCenterPanel showIntro={false} />
              </div>
            </div>
          </>
        );
      case "mcp":
        return (
          <UnifiedMcpPanel onInteractionBlockedChange={setMcpManagementBusy} />
        );
      case "skills":
      case "skillsDiscovery":
        // 「已安装 / 发现」合成一页；旧的 skillsDiscovery 视图直接打开「发现」段
        return (
          <UnifiedSkillsPanel
            initialView={
              currentView === "skillsDiscovery" ? "discover" : "installed"
            }
            onNavigationBlockedChange={setSkillsNavigationBusy}
          />
        );
      case "prompts":
        return (
          <div id="main-content" className="flex min-h-0 flex-1 flex-col">
            <PromptPanel
              appId={promptsApp}
              apps={PROMPT_APP_IDS.filter(
                (app) => visibleApps[app] || app === promptsApp,
              )}
              onAppChange={setPromptsApp}
              onNavigationBlockedChange={setPromptNavigationBusy}
            />
          </div>
        );
      case "sessions":
        // 页头（含 ⋯ 菜单）由会话页自己画：菜单里的操作都在页面状态里
        return (
          <SessionManagerPage
            key={sharedFeatureApp}
            appId={sharedFeatureApp}
            fromApp={activeApp}
            onOpenTerminalSettings={() => openSettings("general")}
          />
        );
      case "apps":
        return <AppsPage />;
      default:
        return null;
    }
  };
  const renderContent = () => {
    if (currentView === "settings") {
      return (
        <SettingsPage
          section={settingsSection}
          appConfigScrollTarget={appConfigScrollTarget}
          onImportSuccess={handleImportSuccess}
          onOpenApps={() => {
            if (confirmLeave(() => setCurrentView("apps"))) return;
            setCurrentView("apps");
          }}
          onOpenApp={selectApp}
        />
      );
    }
    return isAppPage(currentView) ? renderAppPage() : renderGlobalPage();
  };
  return (
    <WindowControlsContext.Provider
      value={useAppWindowControls ? <WindowControls /> : null}
    >
      <div className="flex h-screen overflow-hidden bg-app text-fg-1 selection:bg-action/25">
        <Sidebar
          activeApp={activeApp}
          view={currentView}
          visibleApps={visibleApps}
          settingsSection={settingsSection}
          onSelectApp={selectApp}
          onSelectPage={openPageFromNav}
          onSelectSettingsSection={(section) => {
            setAppConfigScrollTarget(undefined);
            setSettingsSection(section);
          }}
          onExitSettings={exitSettings}
          appsUpdateAvailable={
            checkToolUpdatesOnStartup && toolUpdatesAvailable
          }
        />
        <main
          ref={mainScrollRef}
          id="content-area"
          className="relative isolate flex min-w-0 flex-1 flex-col overflow-hidden bg-app"
        >
          {showEnvCheckFailureBanner && envCheckFailure ? (
            <div className="shrink-0 border-b border-transparent px-6 py-3">
              <Notice
                tone="warning"
                title={t("env.checkFailed.title", {
                  defaultValue: "部分应用的环境变量检查失败",
                })}
                onDismiss={() => {
                  setShowEnvCheckFailureBanner(false);
                  sessionStorage.setItem("env_check_failure_dismissed", "true");
                }}
                dismissLabel={t("common.close")}
                actions={
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      void (async () => {
                        try {
                          const allResults = await checkAllEnvConflicts();
                          const failureSummary =
                            summarizeEnvCheckFailures(allResults);
                          setEnvCheckFailure(failureSummary);
                          setShowEnvCheckFailureBanner(Boolean(failureSummary));
                          const flatConflicts = flattenEnvConflicts(allResults);
                          setEnvConflicts(flatConflicts);
                          if (flatConflicts.length > 0) {
                            setShowEnvBanner(true);
                          }
                        } catch (error) {
                          console.error(
                            "[App] Failed to retry environment conflict check:",
                            error,
                          );
                          setEnvCheckFailure({
                            apps: [],
                            message:
                              extractErrorMessage(error) || String(error),
                          });
                        }
                      })();
                    }}
                  >
                    {t("common.retry")}
                  </Button>
                }
              >
                {envCheckFailure.apps.length > 0
                  ? t("env.checkFailed.description", {
                      apps: envCheckFailure.apps.join(", "),
                      defaultValue:
                        "未检查成功的应用：{{apps}}。这些应用暂时无法确认是否存在冲突环境变量。",
                    })
                  : envCheckFailure.message}
              </Notice>
            </div>
          ) : null}
          {showEnvBanner && envConflicts.length > 0 && (
            <EnvWarningBanner
              conflicts={envConflicts}
              onDismiss={() => {
                setShowEnvBanner(false);
                sessionStorage.setItem("env_banner_dismissed", "true");
              }}
              onDeleted={async () => {
                try {
                  const allResults = await checkAllEnvConflicts();
                  const failureSummary = summarizeEnvCheckFailures(allResults);
                  if (failureSummary) {
                    console.error(
                      "[App] Partial environment conflict check failures:",
                      allResults,
                    );
                    setEnvCheckFailure(failureSummary);
                    setShowEnvCheckFailureBanner(true);
                  } else {
                    setEnvCheckFailure(null);
                    setShowEnvCheckFailureBanner(false);
                  }
                  const flatConflicts = flattenEnvConflicts(allResults);
                  setEnvConflicts(flatConflicts);
                  if (flatConflicts.length === 0) {
                    setShowEnvBanner(false);
                  }
                } catch (error) {
                  console.error(
                    "[App] Failed to re-check conflicts after deletion:",
                    error,
                  );
                  setEnvCheckFailure({
                    apps: [],
                    message: extractErrorMessage(error) || String(error),
                  });
                  setShowEnvCheckFailureBanner(true);
                }
              }}
            />
          )}
          <div
            key={
              currentView === "settings"
                ? "settings"
                : isAppPage(currentView)
                  ? "app"
                  : currentView
            }
            className="flex min-h-0 flex-1 flex-col"
          >
            <Suspense fallback={<PageLoading />}>{renderContent()}</Suspense>
          </div>
        </main>
      </div>

      <Suspense fallback={null}>
        {(isAddOpen || hasOpenedAdd) && (
          <AddProviderDialog
            open={isAddOpen}
            onOpenChange={setIsAddOpen}
            appId={activeApp}
            onSubmit={addProvider}
            modeView={formModeView}
          />
        )}

        <EditProviderHost
          store={editProviderStore}
          appId={activeApp}
          onSubmit={handleEditProvider}
          isProxyTakeover={!!isCurrentAppTakeoverActive}
          currentProviderId={currentProviderId}
        />

        {effectiveUsageProvider && (
          <UsageScriptModal
            key={effectiveUsageProvider.id}
            provider={effectiveUsageProvider}
            appId={activeApp}
            isOpen={Boolean(usageProvider)}
            onClose={() => setUsageProvider(null)}
            onSave={(script) => {
              if (usageProvider) {
                void saveUsageScript(usageProvider, script);
              }
            }}
          />
        )}
      </Suspense>

      <OpenTerminalDialog
        provider={terminalDialog?.provider ?? null}
        appId={terminalDialog?.appId ?? activeApp}
        onClose={() => setTerminalDialog(null)}
      />

      <ConfirmDialog
        isOpen={Boolean(confirmAction)}
        title={
          confirmAction?.action === "remove"
            ? t("confirm.removeProvider")
            : t("confirm.deleteProvider")
        }
        message={confirmActionMessage}
        // 「移除」只是从应用配置里拿掉、CC Switch 里还留着，可以再加回去，不用红色
        variant={confirmAction?.action === "remove" ? "info" : "destructive"}
        pending={confirmActionPending}
        onConfirm={() => void handleConfirmAction()}
        onCancel={() => {
          if (confirmActionPending) return;
          setConfirmAction(null);
        }}
      />

      <ConfirmDialog
        isOpen={pendingLeave !== null}
        title={t("common.unsavedLeaveTitle")}
        message={t("common.unsavedLeaveMessage")}
        confirmText={t("common.unsavedLeaveConfirm")}
        cancelText={t("common.unsavedLeaveCancel")}
        zIndex="top"
        pending={managementBusy}
        onConfirm={() => {
          // 确认框打开后异步操作仍可能变为 busy；导航拒绝时保留草稿登记。
          if (managementBusyRef.current) return;
          const proceed = pendingLeave;
          setPendingLeave(null);
          discardUnsavedChanges();
          proceed?.();
        }}
        onCancel={() => setPendingLeave(null)}
      />

      <DeepLinkImportHost />
      {settingsData && settingsData.firstRunNoticeConfirmed !== true && (
        <Suspense fallback={null}>
          <FirstRunNoticeDialog />
        </Suspense>
      )}
      <NewLayoutDialog />
      <WhatsNewNotice />
    </WindowControlsContext.Provider>
  );
}
export default App;
