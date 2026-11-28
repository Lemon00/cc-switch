import { describe, expect, it } from "vitest";
import type { Provider } from "@/types";
import { findManagedAccountUsers } from "@/lib/managedAccountUsage";
const provider = (
  id: string,
  extra: Partial<Provider> = {},
  settingsConfig: Record<string, unknown> = {},
): Provider =>
  ({
    id,
    name: id,
    settingsConfig,
    ...extra,
  }) as Provider;
const byId = (...list: Provider[]) =>
  Object.fromEntries(list.map((p) => [p.id, p]));
describe("findManagedAccountUsers", () => {
  const claude = byId(
    // 指定了 a1
    provider("bound-a1", {
      meta: {
        providerType: "codex_oauth",
        authBinding: {
          source: "managed_account",
          authProvider: "codex_oauth",
          accountId: "a1",
        },
      },
    }),
    // 旧字段 githubAccountId 指定了 a2
    provider("legacy-a2", {
      meta: {
        providerType: "codex_oauth",
        authBinding: {
          source: "managed_account",
          authProvider: "codex_oauth",
          accountId: "a2",
        },
      },
    }),
    // 没指定：跟着默认账号
    provider("follows-default", { meta: { providerType: "codex_oauth" } }),
    // 没写 providerType 的旧卡按地址认
    provider(
      "by-url",
      {},
      { env: { ANTHROPIC_BASE_URL: "https://chatgpt.com/backend-api/codex" } },
    ),
    // 别的服务
    provider("other", { meta: { providerType: "custom" } }),
    // 普通供应商
    provider("plain", {}, { env: { ANTHROPIC_BASE_URL: "https://x.test" } }),
  );
  it("counts bound providers and default followers for the default account", () => {
    const users = findManagedAccountUsers("codex_oauth", ["a1"], "a1", {
      claude,
    });
    expect(users.map((u) => [u.providerId, u.viaDefault])).toEqual([
      ["bound-a1", false],
      ["follows-default", true],
      ["by-url", true],
    ]);
  });
  it("counts only bound providers for a non-default account", () => {
    const users = findManagedAccountUsers("codex_oauth", ["a2"], "a1", {
      claude,
    });
    expect(users.map((u) => u.providerId)).toEqual(["legacy-a2"]);
  });
  it("collects every provider when removing all accounts", () => {
    const users = findManagedAccountUsers("codex_oauth", ["a1", "a2"], "a1", {
      claude,
    });
    expect(users.map((u) => u.providerId)).toEqual([
      "bound-a1",
      "legacy-a2",
      "follows-default",
      "by-url",
    ]);
  });
});
