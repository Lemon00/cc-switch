import type { ManagedAuthProvider } from "@/lib/api/auth";
import {
  findManagedAccountUsers,
  type ManagedAccountUser,
} from "@/lib/managedAccountUsage";
import { useProvidersQuery } from "@/lib/query";
import { useCallback } from "react";
/**
 * 授权中心的账号被哪些供应商在用：用前端已有的供应商列表（和供应商页同一份缓存）现算。
 */
export function useManagedAccountUsers(
  authProvider: ManagedAuthProvider,
  defaultAccountId: string | null,
) {
  const { data: claude } = useProvidersQuery("claude");
  const { data: codex } = useProvidersQuery("codex");
  const { data: agy } = useProvidersQuery("agy");
  return useCallback(
    (accountIds: readonly string[]): ManagedAccountUser[] =>
      findManagedAccountUsers(authProvider, accountIds, defaultAccountId, {
        claude: claude?.providers,
        codex: codex?.providers,
        agy: agy?.providers,
      }),
    [authProvider, defaultAccountId, claude, codex, agy],
  );
}
