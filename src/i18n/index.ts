import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import zh from "./locales/zh.json";
import {
  normalizeLanguage,
  supportedLanguage,
  type Language,
} from "./language";
export type { Language } from "./language";

function getInitialLanguage(): Language {
  try {
    if (typeof window !== "undefined") {
      const stored = supportedLanguage(window.localStorage.getItem("language"));
      if (stored) return stored;
    }
  } catch (error) {
    console.warn("[i18n] Failed to read stored language preference", error);
  }
  return normalizeLanguage(
    typeof navigator !== "undefined"
      ? navigator.language || navigator.languages?.[0]
      : undefined,
  );
}

i18n.use(initReactI18next).init({
  resources: { zh: { translation: zh }, en: { translation: en } },
  lng: getInitialLanguage(),
  supportedLngs: ["zh", "en"],
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  debug: false,
});

/** 后端设置是语言偏好的来源，localStorage 仅作为下次启动的缓存。 */
export const syncLanguageFromSettings = (language?: string | null): void => {
  if (!language) return;
  const normalized = normalizeLanguage(language);
  if (i18n.language !== normalized) void i18n.changeLanguage(normalized);
  try {
    if (typeof window !== "undefined") {
      window.localStorage.setItem("language", normalized);
    }
  } catch (error) {
    console.warn("[i18n] Failed to persist language preference", error);
  }
};

export default i18n;
