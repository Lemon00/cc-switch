import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DatabaseUpgrade } from "./components/DatabaseUpgrade";
import { UpdateProvider } from "./contexts/UpdateContext";
import "./index.css";
// 导入国际化配置
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { settingsApi } from "@/lib/api";
import { disableContextMenu } from "@/lib/disableContextMenu";
import { initializeInputModality } from "@/lib/inputModality";
import { queryClient } from "@/lib/query";
import { localUsageQueryFilters } from "@/lib/query/usage";
import { initializeWindowActivity } from "@/lib/windowActivity";
import { QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message } from "@tauri-apps/plugin-dialog";
import { exit } from "@tauri-apps/plugin-process";
import { MotionConfig } from "framer-motion";
import { FrontendErrorBoundary } from "./components/FrontendErrorBoundary";
import i18n, { syncLanguageFromSettings } from "./i18n";
import {
  installGlobalErrorHandlers,
  reportFrontendError,
} from "./lib/frontendLogger";
import {
  MODELS_DEV_SYNC_CONFIG_QUERY_KEY,
  syncModelsDevPricingOnStartup,
} from "./lib/modelsDevAutoSync";
installGlobalErrorHandlers();
disableContextMenu();
// 根据平台添加 body class，便于平台特定样式
try {
  const ua = navigator.userAgent || "";
  const plat = (navigator.platform || "").toLowerCase();
  const isMac = /mac/i.test(ua) || plat.includes("mac");
  if (isMac) {
    document.body.classList.add("is-mac");
  }
} catch {
  // 忽略平台检测失败
}
const root = ReactDOM.createRoot(document.getElementById("root")!);
let fatalStartupError = false;
// 配置加载错误payload类型
interface ConfigLoadErrorPayload {
  path?: string;
  error?: string;
  /** "db_version_too_new" 表示数据库版本过新，渲染应用内升级恢复界面 */
  kind?: string;
}
/**
 * 处理配置加载失败：显示错误消息并强制退出应用
 * 不给用户"取消"选项，因为配置损坏时应用无法正常运行
 */
async function handleConfigLoadError(
  payload: ConfigLoadErrorPayload | null,
): Promise<void> {
  fatalStartupError = true;
  const path = payload?.path ?? "~/.cc-switch/config.json";
  const detail = payload?.error ?? "Unknown error";
  // 保留可见的失败状态；原生弹框或退出 IPC 失败也不能进入业务 App。
  root.render(
    <main role="alert" className="p-8 space-y-4">
      <h1>
        {i18n.t("errors.startupFailedTitle", {
          defaultValue: "Application could not start",
        })}
      </h1>
      <p>{path}</p>
      <pre className="whitespace-pre-wrap break-words">{detail}</pre>
    </main>,
  );
  try {
    await message(
      i18n.t(
        payload?.kind
          ? "errors.startupFailedMessage"
          : "errors.configLoadFailedMessage",
        {
          path,
          detail,
          defaultValue: payload?.kind
            ? "应用初始化失败：\n{{path}}\n\n{{detail}}\n\n请修复后重新启动应用。"
            : "无法读取配置文件：\n{{path}}\n\n错误详情：\n{{detail}}\n\n请手动检查 JSON 是否有效，或从同目录的备份文件（如 config.json.bak）恢复。\n\n应用将退出以便您进行修复。",
        },
      ),
      {
        title: i18n.t(
          payload?.kind
            ? "errors.startupFailedTitle"
            : "errors.configLoadFailedTitle",
          {
            defaultValue: "配置加载失败",
          },
        ),
        kind: "error",
      },
    );
  } catch (error) {
    reportFrontendError("startup_error_dialog", error);
  }
  try {
    await exit(1);
  } catch (error) {
    reportFrontendError("startup_error_exit", error);
  }
}
// 监听后端的配置加载错误事件：仅提醒用户并强制退出，不修改任何配置文件
void listen("configLoadError", async (evt) => {
  await handleConfigLoadError(evt.payload as ConfigLoadErrorPayload | null);
}).catch((e) => {
  // 非 Tauri 环境或 IPC 未就绪时 listen 会 reject；同步 try/catch 捕不到
  reportFrontendError("config_load_error_listener", e);
});
async function bootstrap() {
  root.render(<div role="status">{i18n.t("common.loading")}</div>);
  let initError: ConfigLoadErrorPayload | null;
  try {
    initError = await invoke<ConfigLoadErrorPayload | null>("get_init_error");
  } catch (error) {
    reportFrontendError("get_init_error", error);
    // 无法确认后端状态时也不挂载会调用业务 IPC 的组件。
    initError = { error: String(error), kind: "startup_check_failed" };
  }
  if (fatalStartupError) return;
  if (initError?.kind === "db_version_too_new") {
    root.render(
      <React.StrictMode>
        <FrontendErrorBoundary>
          <ThemeProvider defaultTheme="system" storageKey="cc-switch-theme">
            <DatabaseUpgrade payload={initError} />
            <Toaster />
          </ThemeProvider>
        </FrontendErrorBoundary>
      </React.StrictMode>,
    );
    return;
  }
  if (initError) {
    await handleConfigLoadError(initError);
    return;
  }
  initializeWindowActivity();
  initializeInputModality();
  // 首屏语言来自 localStorage 缓存；拿到后端保存的语言后再校正一次。
  // 与 useSettingsQuery 共用 ["settings"] 缓存，进设置页不会重复拉取。
  void queryClient
    .fetchQuery({ queryKey: ["settings"], queryFn: () => settingsApi.get() })
    .then((settings) => syncLanguageFromSettings(settings.language))
    .catch((error) => reportFrontendError("startup_language_sync", error));
  root.render(
    <React.StrictMode>
      <FrontendErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <ThemeProvider defaultTheme="system" storageKey="cc-switch-theme">
            <UpdateProvider>
              {/* 系统开了「减少动态效果」时，framer-motion 的位移动画一律跳过 */}
              <MotionConfig reducedMotion="user">
                {/* 全局悬停提示：移上去立即出现（HoverTip） */}
                <TooltipProvider delayDuration={0} skipDelayDuration={0}>
                  <App />
                </TooltipProvider>
              </MotionConfig>
              <Toaster />
            </UpdateProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </FrontendErrorBoundary>
    </React.StrictMode>,
  );
  void syncModelsDevPricingOnStartup()
    .then((result) => {
      if (!result.skipped) {
        return Promise.all([
          queryClient.invalidateQueries(localUsageQueryFilters),
          queryClient.invalidateQueries({
            queryKey: MODELS_DEV_SYNC_CONFIG_QUERY_KEY,
          }),
        ]);
      }
    })
    .catch((error) => {
      // 离线或 models.dev 暂时不可用不应阻塞应用启动。
      reportFrontendError("models_dev_startup_sync", error);
      void queryClient.invalidateQueries({
        queryKey: MODELS_DEV_SYNC_CONFIG_QUERY_KEY,
      });
    });
}
void bootstrap();
