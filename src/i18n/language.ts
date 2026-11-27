export type Language = "zh" | "en";

export function supportedLanguage(value?: string | null): Language | undefined {
  const language = value?.trim().toLowerCase().split(/[-_]/)[0];
  return language === "zh" || language === "en" ? language : undefined;
}

export function normalizeLanguage(value?: string | null): Language {
  return supportedLanguage(value) ?? (value ? "en" : "zh");
}
