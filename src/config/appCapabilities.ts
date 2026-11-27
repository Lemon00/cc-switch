import type { AppId } from "@/lib/api/types";
import type { VisibleApps } from "@/types";
export const APP_IDS: AppId[] = ["claude", "codex", "opencode", "dsh", "agy"];
export const DEFAULT_VISIBLE_APPS: VisibleApps = {
  claude: true,
  codex: true,
  opencode: true,
  dsh: true,
  agy: true,
};
/**
 * 应用页和侧栏的应用顺序：先按用户拖出来的顺序（`settings.appOrder`，不认识的 ID 忽略），
 * 没在里面的应用（比如新版本加的）插回默认顺序里它前一个应用的后面。
 */
export function orderAppIds(saved: readonly string[] | undefined): AppId[] {
  if (!saved?.length) return [...APP_IDS];
  const known = new Set<string>(APP_IDS);
  const ordered: AppId[] = [];
  for (const id of saved) {
    if (known.has(id) && !ordered.includes(id as AppId)) {
      ordered.push(id as AppId);
    }
  }
  APP_IDS.forEach((app, index) => {
    if (ordered.includes(app)) return;
    const previous = APP_IDS.slice(0, index)
      .reverse()
      .find((id) => ordered.includes(id));
    ordered.splice(previous ? ordered.indexOf(previous) + 1 : 0, 0, app);
  });
  return ordered;
}
/** App IDs shown in Skills panels. */
export const SKILLS_APP_IDS: AppId[] = [
  "claude",
  "codex",
  "opencode",
  "dsh",
  "agy",
];
export type ProxyAppId = "claude" | "codex";
/** Apps with a complete local gateway + failover data plane. */
export const PROXY_APP_IDS: ProxyAppId[] = ["claude", "codex"];
export function isProxyAppId(appId: string): appId is ProxyAppId {
  return (PROXY_APP_IDS as string[]).includes(appId);
}
/**
 * 当前发行版提供直连 / 路由模式的应用。旧类型仅用于兼容已有配置。
 */
export type ModeAppId = ProxyAppId;
export const MODE_APP_IDS: ModeAppId[] = ["claude", "codex"];
export function isModeAppId(appId: string): appId is ModeAppId {
  return (MODE_APP_IDS as string[]).includes(appId);
}
/** 支持 Stack 模式的应用（后端 `mode::stack::supports_stack` 的镜像）。 */
export type StackAppId = ProxyAppId;
export const STACK_APP_IDS: StackAppId[] = ["claude", "codex"];
export function isStackAppId(appId: string): appId is StackAppId {
  return (STACK_APP_IDS as string[]).includes(appId);
}
export type AdditiveAppId = "opencode" | "dsh";
export const ADDITIVE_APP_IDS: AdditiveAppId[] = ["opencode", "dsh"];
export function isAdditiveAppId(appId: string): appId is AdditiveAppId {
  return (ADDITIVE_APP_IDS as string[]).includes(appId);
}
/**
 * 切换只替换关键字段的应用：供应商编辑器显示「切到这个供应商之后配置文件的样子」，由后端
 * `ProviderService::editor_view` 投影。
 */
export const EDITOR_VIEW_APP_IDS: AppId[] = ["claude", "codex"];
export function usesEditorView(appId: AppId): boolean {
  return EDITOR_VIEW_APP_IDS.includes(appId);
}
/** Legacy MCP identities remain valid for saved data; the UI uses MCP_APP_IDS. */
export type McpAppId = AppId;
export const MCP_APP_IDS: McpAppId[] = [
  "claude",
  "codex",
  "opencode",
  "dsh",
  "agy",
];
export function isMcpAppId(appId: string): appId is McpAppId {
  return (MCP_APP_IDS as string[]).includes(appId);
}
export function usesAgyForm(appId: string): boolean {
  return appId === "agy";
}
/** Native catalogs that are reconciled on activation and window focus. */
export const NATIVE_PROVIDER_APP_IDS: readonly AppId[] = ["dsh"];
export const hasNativeProviderCatalog = (app: AppId): boolean => app === "dsh";
export const APP_LABELS: Record<AppId, string> = {
  claude: "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  dsh: "DeepSeek Harness",
  agy: "Antigravity",
};
export function getAppLabel(appId: string): string {
  return APP_LABELS[appId as AppId] ?? appId;
}
/** Product support is independent of saved visibility preferences. */
export function isSupportedAppId(value: string): value is AppId {
  return (APP_IDS as readonly string[]).includes(value);
}
