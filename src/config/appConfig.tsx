import { ClaudeIcon, CodexIcon } from "@/components/BrandIcons";
import { ProviderIcon } from "@/components/ProviderIcon";
import type { AppId } from "@/lib/api/types";
import React from "react";
import { APP_LABELS } from "./appCapabilities";
export interface AppConfig {
  label: string;
  icon: React.ReactNode;
  activeClass: string;
  badgeClass: string;
}
export * from "./appCapabilities";
export const APP_ICON_MAP: Record<AppId, AppConfig> = {
  claude: {
    label: APP_LABELS["claude"],
    icon: <ClaudeIcon size={14} />,
    activeClass:
      "bg-orange-500/10 ring-1 ring-orange-500/20 hover:bg-orange-500/20 text-orange-600 dark:text-orange-400",
    badgeClass:
      "bg-orange-500/10 text-orange-700 dark:text-orange-300 hover:bg-orange-500/20 border-0 gap-1.5",
  },
  codex: {
    label: APP_LABELS["codex"],
    icon: <CodexIcon size={14} />,
    activeClass:
      "bg-green-500/10 ring-1 ring-green-500/20 hover:bg-green-500/20 text-green-600 dark:text-green-400",
    badgeClass:
      "bg-green-500/10 text-green-700 dark:text-green-300 hover:bg-green-500/20 border-0 gap-1.5",
  },
  opencode: {
    label: APP_LABELS["opencode"],
    icon: (
      <ProviderIcon
        icon="opencode"
        name="OpenCode"
        size={14}
        showFallback={false}
      />
    ),
    activeClass:
      "bg-indigo-500/10 ring-1 ring-indigo-500/20 hover:bg-indigo-500/20 text-indigo-600 dark:text-indigo-400",
    badgeClass:
      "bg-indigo-500/10 text-indigo-700 dark:text-indigo-300 hover:bg-indigo-500/20 border-0 gap-1.5",
  },
  dsh: {
    label: APP_LABELS["dsh"],
    icon: <ProviderIcon icon="deepseek" name="DeepSeek Harness" size={14} />,
    activeClass: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    badgeClass:
      "bg-blue-500/10 text-blue-700 dark:text-blue-300 border-0 gap-1.5",
  },
  agy: {
    label: APP_LABELS["agy"],
    icon: (
      <ProviderIcon
        icon="antigravity"
        name="Antigravity"
        size={14}
        showFallback={false}
      />
    ),
    activeClass:
      "bg-sky-500/10 ring-1 ring-sky-500/20 hover:bg-sky-500/20 text-sky-700 dark:text-sky-300",
    badgeClass:
      "bg-sky-500/10 text-sky-700 dark:text-sky-300 hover:bg-sky-500/20 border-0 gap-1.5",
  },
};
