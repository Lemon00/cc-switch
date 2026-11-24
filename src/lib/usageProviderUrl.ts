export function parseUsageProviderUrl(
  baseUrl: string | null | undefined,
): URL | null {
  if (!baseUrl) return null;
  try {
    const input = baseUrl.trim();
    const parsed = new URL(input.includes("://") ? input : `https://${input}`);
    return ["http:", "https:"].includes(parsed.protocol) && parsed.hostname
      ? parsed
      : null;
  } catch {
    return null;
  }
}
