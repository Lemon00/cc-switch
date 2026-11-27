import { describe, expect, it } from "vitest";
import {
  APP_IDS,
  DEFAULT_VISIBLE_APPS,
  MCP_APP_IDS,
  SKILLS_APP_IDS,
  PROXY_APP_IDS,
  MODE_APP_IDS,
  STACK_APP_IDS,
  ADDITIVE_APP_IDS,
  NATIVE_PROVIDER_APP_IDS,
  isSupportedAppId,
  orderAppIds,
} from "@/config/appCapabilities";
import { DIRECTORY_APPS } from "@/config/directoryApps";
import { SESSION_APP_IDS } from "@/lib/sessions/search";
import { KNOWN_APP_TYPES } from "@/types/usage";
describe("supported application contract", () => {
  const supported = ["claude", "codex", "opencode", "dsh", "agy"];
  const retired = [
    "gemini",
    "grokbuild",
    "claude-desktop",
    "openclaw",
    "hermes",
    "pi",
    "mcode",
    "kimi",
    "omp",
  ];
  it("exposes the same five applications for navigation, MCP, Skills, sessions and usage", () => {
    for (const ids of [
      APP_IDS,
      MCP_APP_IDS,
      SKILLS_APP_IDS,
      SESSION_APP_IDS,
      KNOWN_APP_TYPES,
    ]) {
      expect(ids).toEqual(supported);
    }
    for (const ids of [
      PROXY_APP_IDS,
      MODE_APP_IDS,
      STACK_APP_IDS,
      ADDITIVE_APP_IDS,
      NATIVE_PROVIDER_APP_IDS,
      DIRECTORY_APPS.map((app) => app.app),
    ]) {
      expect(ids.every(isSupportedAppId)).toBe(true);
    }
  });
  it("never restores retired applications from a saved order or old visibility flags", () => {
    expect(orderAppIds([...retired, "codex", "codex"])).toEqual(supported);
    for (const id of retired) {
      expect(isSupportedAppId(id)).toBe(false);
      expect(
        !!DEFAULT_VISIBLE_APPS[id as keyof typeof DEFAULT_VISIBLE_APPS],
      ).toBe(false);
    }
    expect(supported.every(isSupportedAppId)).toBe(true);
  });
});
