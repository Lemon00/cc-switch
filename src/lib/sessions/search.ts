import { isSupportedAppId } from "@/config/appCapabilities";
import type { AppId } from "@/lib/api/types";
/** 会话来源应用（Claude Desktop 没有自己的会话记录，用 Claude Code 的）。 */
const LEGACY_SESSION_APP_IDS = [
  "claude",
  "codex",
  "opencode",
  "dsh",
  "agy",
] as const satisfies readonly AppId[];
export const SESSION_APP_IDS = LEGACY_SESSION_APP_IDS.filter(isSupportedAppId);
export type SessionAppId = (typeof LEGACY_SESSION_APP_IDS)[number];
export const isSessionAppId = (value: string): value is SessionAppId =>
  (SESSION_APP_IDS as readonly string[]).includes(value);
export const countMatches = (text: string, query: string) => {
  const needle = query.trim().toLowerCase();
  if (!needle) return 0;
  const haystack = text.toLowerCase();
  let count = 0;
  let at = haystack.indexOf(needle);
  while (at >= 0) {
    count += 1;
    at = haystack.indexOf(needle, at + needle.length);
  }
  return count;
};
