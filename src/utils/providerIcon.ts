import type { AppId } from "@/lib/api";

/** Preserve the provider's chosen icon independently of its client app. */
export function resolveProviderIcon(
  _appId: AppId,
  icon?: string,
  _iconColor?: string,
): string | undefined {
  const normalizedIcon = icon?.trim();
  if (!normalizedIcon) return undefined;
  return normalizedIcon;
}
