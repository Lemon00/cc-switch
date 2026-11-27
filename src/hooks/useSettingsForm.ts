import {
  DIRECTORY_APPS,
  sanitizeDir,
  sanitizeDirectoryFields,
} from "@/config/directoryApps";
import { useSettingsQuery } from "@/lib/query";
import type { Settings } from "@/types";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  normalizeLanguage,
  supportedLanguage,
  type Language,
} from "@/i18n/language";
function sameSettingValue(
  key: keyof Settings,
  left: unknown,
  right: unknown,
): boolean {
  if (DIRECTORY_APPS.some(({ field }) => field === key)) {
    return (
      sanitizeDir(left as string | undefined) ===
      sanitizeDir(right as string | undefined)
    );
  }
  return JSON.stringify(left) === JSON.stringify(right);
}
export type SettingsFormState = Omit<Settings, "language"> & {
  language: Language;
};
export interface UseSettingsFormResult {
  settings: SettingsFormState | null;
  isLoading: boolean;
  initialLanguage: Language;
  updateSettings: (updates: Partial<SettingsFormState>) => void;
  resetSettings: (serverData: Settings | null) => void;
  readPersistedLanguage: () => Language;
  syncLanguage: (lang: Language) => void;
}
/**
 * useSettingsForm - 表单状态管理
 * 负责：
 * - 表单数据状态
 * - 表单字段更新
 * - 语言同步
 * - 表单重置
 */
export function useSettingsForm(): UseSettingsFormResult {
  const { i18n } = useTranslation();
  const { data, isLoading } = useSettingsQuery();
  const [settingsState, setSettingsState] = useState<SettingsFormState | null>(
    null,
  );
  const initialLanguageRef = useRef<Language>("zh");
  const savedSettingsRef = useRef<SettingsFormState | null>(null);
  const draftsRef = useRef(new Map<keyof SettingsFormState, unknown>());
  const readPersistedLanguage = useCallback((): Language => {
    try {
      if (typeof window !== "undefined") {
        const stored = supportedLanguage(
          window.localStorage.getItem("language"),
        );
        if (stored) return stored;
      }
    } catch {
      // 缓存不可读时沿用当前界面语言。
    }
    return normalizeLanguage(i18n.language);
  }, [i18n]);
  const syncLanguage = useCallback(
    (lang: Language) => {
      const current = i18n.language;
      if (current !== lang) {
        void i18n.changeLanguage(lang);
      }
    },
    [i18n],
  );
  // 初始化设置数据
  useEffect(() => {
    if (!data) return;
    const normalizedLanguage = normalizeLanguage(
      data.language ?? readPersistedLanguage(),
    );
    const normalized: SettingsFormState = {
      ...data,
      showInTray: data.showInTray ?? true,
      minimizeToTrayOnClose: data.minimizeToTrayOnClose ?? true,
      useAppWindowControls: data.useAppWindowControls ?? false,
      enableClaudePluginIntegration:
        data.enableClaudePluginIntegration ?? false,
      silentStartup: data.silentStartup ?? false,
      skipClaudeOnboarding: data.skipClaudeOnboarding ?? false,
      preserveCodexOfficialAuthOnSwitch:
        data.preserveCodexOfficialAuthOnSwitch ?? false,
      unifyCodexSessionHistory: data.unifyCodexSessionHistory ?? false,
      codexStackClassicTools: data.codexStackClassicTools ?? true,
      ...sanitizeDirectoryFields(data),
      language: normalizedLanguage,
    };
    for (const [key, value] of draftsRef.current) {
      if (sameSettingValue(key, value, normalized[key])) {
        draftsRef.current.delete(key);
      }
    }
    const refreshed = {
      ...normalized,
      ...Object.fromEntries(draftsRef.current),
    };
    savedSettingsRef.current = normalized;
    setSettingsState(refreshed);
    initialLanguageRef.current = normalizedLanguage;
    syncLanguage(refreshed.language);
  }, [data, readPersistedLanguage, syncLanguage]);
  const updateSettings = useCallback(
    (updates: Partial<SettingsFormState>) => {
      for (const [field, rawValue] of Object.entries(updates)) {
        const key = field as keyof SettingsFormState;
        const value =
          key === "language" ? normalizeLanguage(rawValue as string) : rawValue;
        if (sameSettingValue(key, value, savedSettingsRef.current?.[key])) {
          draftsRef.current.delete(key);
        } else {
          draftsRef.current.set(key, value);
        }
      }
      setSettingsState((prev) => {
        const base =
          prev ??
          ({
            showInTray: true,
            minimizeToTrayOnClose: true,
            useAppWindowControls: false,
            enableClaudePluginIntegration: false,
            skipClaudeOnboarding: false,
            preserveCodexOfficialAuthOnSwitch: false,
            unifyCodexSessionHistory: false,
            codexStackClassicTools: true,
            language: readPersistedLanguage(),
          } as SettingsFormState);
        const next: SettingsFormState = {
          ...base,
          ...updates,
        };
        if (updates.language) {
          const normalized = normalizeLanguage(updates.language);
          next.language = normalized;
          syncLanguage(normalized);
        }
        return next;
      });
    },
    [readPersistedLanguage, syncLanguage],
  );
  const resetSettings = useCallback(
    (serverData: Settings | null) => {
      if (!serverData) return;
      const normalizedLanguage = normalizeLanguage(
        serverData.language ?? readPersistedLanguage(),
      );
      const normalized: SettingsFormState = {
        ...serverData,
        showInTray: serverData.showInTray ?? true,
        minimizeToTrayOnClose: serverData.minimizeToTrayOnClose ?? true,
        useAppWindowControls: serverData.useAppWindowControls ?? false,
        enableClaudePluginIntegration:
          serverData.enableClaudePluginIntegration ?? false,
        silentStartup: serverData.silentStartup ?? false,
        skipClaudeOnboarding: serverData.skipClaudeOnboarding ?? false,
        preserveCodexOfficialAuthOnSwitch:
          serverData.preserveCodexOfficialAuthOnSwitch ?? false,
        unifyCodexSessionHistory: serverData.unifyCodexSessionHistory ?? false,
        codexStackClassicTools: serverData.codexStackClassicTools ?? true,
        ...sanitizeDirectoryFields(serverData),
        language: normalizedLanguage,
      };
      draftsRef.current.clear();
      savedSettingsRef.current = normalized;
      setSettingsState(normalized);
      initialLanguageRef.current = normalizedLanguage;
      syncLanguage(normalizedLanguage);
    },
    [readPersistedLanguage, syncLanguage],
  );
  return {
    settings: settingsState,
    isLoading,
    initialLanguage: initialLanguageRef.current,
    updateSettings,
    resetSettings,
    readPersistedLanguage,
    syncLanguage,
  };
}
