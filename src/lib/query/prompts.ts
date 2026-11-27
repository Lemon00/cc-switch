import { promptsApi, type AppId, type Prompt } from "@/lib/api";
import { useQuery } from "@tanstack/react-query";
/**
 * 提示词页之外读提示词的地方（应用页页头的「提示词：X ›」、提示词页应用下拉里的条数）。
 * 提示词页自己的列表走 usePromptActions；写完后 invalidate `promptKeys.all`，这里跟着刷新。
 */
export const promptKeys = {
  all: ["prompts"] as const,
  list: (app: AppId | "__idle__") => ["prompts", "list", app] as const,
  location: (app: AppId) => ["prompts", "location", app] as const,
};
/** 支持提示词的应用，按侧栏顺序。Claude Desktop 用 Claude Code 的；OpenClaw 在自己的「工作区」里管理。 */
export const PROMPT_APP_IDS: AppId[] = [
  "claude",
  "codex",
  "opencode",
  "dsh",
  "agy",
];
/** 应用页对应的提示词应用；不支持提示词的返回 null。 */
export function promptAppOf(app: AppId): AppId | null {
  return PROMPT_APP_IDS.includes(app) ? app : null;
}
export function usePromptListQuery(app: AppId | null, enabled = true) {
  return useQuery({
    // idle 时用哨兵 key，避免多处 app=null 共享 claude 缓存槽
    queryKey: promptKeys.list(app ?? "__idle__"),
    queryFn: () => promptsApi.getPrompts(app as AppId),
    enabled: enabled && app !== null,
  });
}
export function usePromptFileLocationQuery(app: AppId) {
  return useQuery({
    queryKey: promptKeys.location(app),
    queryFn: () => promptsApi.getFileLocation(app),
    staleTime: 30000,
  });
}
export function enabledPromptOf(
  prompts: Record<string, Prompt> | undefined,
): Prompt | undefined {
  if (!prompts) return undefined;
  return Object.values(prompts).find((prompt) => prompt.enabled);
}
