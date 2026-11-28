import type { AppId } from "@/lib/api";
import type { ProviderCategory } from "@/types";
import { useCallback, useEffect, useRef, useState } from "react";
interface UseBaseUrlStateProps {
  appType: AppId;
  category: ProviderCategory | undefined;
  settingsConfig: string;
  onSettingsConfigChange: (config: string) => void;
  /**
   * 现读当前配置。`settingsConfig` 是调用方在渲染期取的快照，可能早于别处
   * （如 JSON 编辑器）的写入。改 Base URL 时若基于旧快照重建整份 JSON，就会
   * 把那些改动覆盖回去。传入本 getter 即可在事件发生时现读。
   */
  getSettingsConfig?: () => string;
}
/** 读 settingsConfig 里 env 下的地址；解析不了时为空 */
function readEnvUrl(settingsConfig: string, key: string): string {
  try {
    const envUrl: unknown = JSON.parse(settingsConfig || "{}")?.env?.[key];
    return typeof envUrl === "string" ? envUrl.trim() : "";
  } catch {
    return "";
  }
}
/**
 * 管理 Base URL 状态
 * Claude 的 JSON 配置地址；Codex 和 Antigravity 由各自配置 hook 管理。
 */
export function useBaseUrlState({
  appType,
  category,
  settingsConfig,
  onSettingsConfigChange,
  getSettingsConfig,
}: UseBaseUrlStateProps) {
  // 初始值和下面的同步 effect 读法一致，打开编辑页时不用等 effect 回填、多渲染一轮
  const [baseUrl, setBaseUrl] = useState(() =>
    appType === "claude" && category !== "official"
      ? readEnvUrl(settingsConfig, "ANTHROPIC_BASE_URL")
      : "",
  );
  const isUpdatingRef = useRef(false);
  // 事件处理里一律走这里取配置：有 getter 用 getter（当前值），否则退回快照。
  const readSettingsConfig = useCallback(
    () => getSettingsConfig?.() ?? settingsConfig,
    [getSettingsConfig, settingsConfig],
  );
  // 从配置同步到 state（Claude / Claude Desktop）
  useEffect(() => {
    if (appType !== "claude") return;
    // 只有 official 类别不显示 Base URL 输入框，其他类别都需要回填
    if (category === "official") return;
    if (isUpdatingRef.current) return;
    try {
      const config = JSON.parse(settingsConfig || "{}");
      const envUrl: unknown = config?.env?.ANTHROPIC_BASE_URL;
      const nextUrl = typeof envUrl === "string" ? envUrl.trim() : "";
      if (nextUrl !== baseUrl) {
        setBaseUrl(nextUrl);
      }
    } catch {
      // ignore
    }
  }, [appType, category, settingsConfig, baseUrl]);

  // 处理 Claude Base URL 变化
  const handleClaudeBaseUrlChange = useCallback(
    (url: string) => {
      const sanitized = url.trim();
      setBaseUrl(sanitized);
      isUpdatingRef.current = true;
      try {
        const config = JSON.parse(readSettingsConfig() || "{}");
        if (!config.env) {
          config.env = {};
        }
        config.env.ANTHROPIC_BASE_URL = sanitized;
        onSettingsConfigChange(JSON.stringify(config, null, 2));
      } catch {
        // ignore
      } finally {
        setTimeout(() => {
          isUpdatingRef.current = false;
        }, 0);
      }
    },
    [readSettingsConfig, onSettingsConfigChange],
  );
  return {
    baseUrl,
    handleClaudeBaseUrlChange,
  };
}
