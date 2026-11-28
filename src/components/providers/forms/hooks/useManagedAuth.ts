import type {
  ManagedAuthDeviceCodeResponse,
  ManagedAuthProvider,
  ManagedAuthStatus,
} from "@/lib/api";
import { authApi } from "@/lib/api";
import type { CodexLoginMethod, ManagedLoginMethod } from "@/lib/api/auth";
import { CODEX_OAUTH_DUPLICATE_ACCOUNT_ERROR } from "@/lib/api/auth";
import { deliverLoginLink, readAuthOpenBrowser } from "@/lib/authLoginLink";
import { copyText } from "@/lib/clipboard";
import { subscriptionKeys } from "@/lib/query/subscription";
import { toast } from "@/lib/toast";
import { extractErrorMessage } from "@/utils/errorUtils";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
/** Codex 凭据已清、供应商解绑失败时的稳定前缀（见 codex_oauth_switch）。 */
const CREDENTIALS_CLEARED_PREFIX = "CREDENTIALS_CLEARED:";
type AccountMutationResult =
  | {
      kind: "ok";
    }
  | {
      kind: "partial";
      message: string;
    };
function parseCredentialsClearedError(error: unknown): string | null {
  const message = extractErrorMessage(error);
  if (!message.startsWith(CREDENTIALS_CLEARED_PREFIX)) return null;
  return message.slice(CREDENTIALS_CLEARED_PREFIX.length).trim() || message;
}
type PollingState = "idle" | "polling" | "success" | "error";
type LoginRequest = {
  targetAccountId?: string;
  generation: number;
  loginMethod?: ManagedLoginMethod;
};
export function useManagedAuth(
  authProvider: ManagedAuthProvider,
  loginMethod?: CodexLoginMethod,
) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const queryKey = ["managed-auth-status", authProvider];
  const [pollingState, setPollingState] = useState<PollingState>("idle");
  const [deviceCode, setDeviceCode] =
    useState<ManagedAuthDeviceCodeResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pollingIntervalRef = useRef<ReturnType<typeof setInterval> | null>(
    null,
  );
  const pollingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const flowGenerationRef = useRef(0);
  const activeDeviceCodeRef = useRef<string | null>(null);
  const retryTargetAccountIdRef = useRef<string | undefined>(undefined);
  const flowTransitionRef = useRef<Promise<void>>(Promise.resolve());
  const {
    data: authStatus,
    isLoading: isLoadingStatus,
    isSuccess: isStatusSuccess,
    isError: isStatusError,
    refetch: refetchStatus,
  } = useQuery<ManagedAuthStatus>({
    queryKey,
    queryFn: () => authApi.authGetStatus(authProvider),
    staleTime: 30000,
    refetchInterval: false,
  });
  const stopPolling = useCallback(() => {
    if (pollingIntervalRef.current) {
      clearInterval(pollingIntervalRef.current);
      pollingIntervalRef.current = null;
    }
    if (pollingTimeoutRef.current) {
      clearTimeout(pollingTimeoutRef.current);
      pollingTimeoutRef.current = null;
    }
  }, []);
  const cancelBackendFlow = useCallback(
    async (deviceCode: string | null): Promise<boolean> => {
      // Every provider must invalidate an in-flight token exchange on cancel.
      if (!deviceCode) return true;
      try {
        const cancelled = await authApi.authCancelLogin(
          authProvider,
          deviceCode,
        );
        if (!cancelled) {
          await queryClient.invalidateQueries({
            queryKey: ["managed-auth-status", authProvider],
          });
        }
        return cancelled;
      } catch (e) {
        console.debug("[ManagedAuth] Failed to cancel device flow:", e);
        await queryClient.invalidateQueries({
          queryKey: ["managed-auth-status", authProvider],
        });
        return false;
      }
    },
    [authProvider, queryClient],
  );
  const queueBackendCancellation = useCallback(
    (deviceCode: string | null) => {
      const transition = flowTransitionRef.current.then(async () => {
        await cancelBackendFlow(deviceCode);
      });
      flowTransitionRef.current = transition;
      return transition;
    },
    [cancelBackendFlow],
  );
  useEffect(() => {
    return () => {
      flowGenerationRef.current += 1;
      void cancelBackendFlow(activeDeviceCodeRef.current);
      activeDeviceCodeRef.current = null;
      stopPolling();
    };
  }, [cancelBackendFlow, stopPolling]);
  const startLoginMutation = useMutation({
    mutationFn: ({ targetAccountId, loginMethod: method }: LoginRequest) =>
      method
        ? authApi.authStartLogin(authProvider, targetAccountId, method)
        : authApi.authStartLogin(authProvider, targetAccountId),
    onSuccess: async (response, request) => {
      if (request.generation !== flowGenerationRef.current) {
        void cancelBackendFlow(response.device_code);
        return;
      }
      activeDeviceCodeRef.current = response.device_code;
      setDeviceCode(response);
      setPollingState("polling");
      setError(null);
      // 按授权中心的偏好打开浏览器，或只复制登录链接
      const linkOnly = response.browser_opened
        ? false
        : (await deliverLoginLink(response.verification_uri, t)).linkOnly;
      if (request.generation !== flowGenerationRef.current) return;
      // Browser login has no verification code; leave the clipboard untouched.
      // 只复制了链接时也不复制验证码，免得盖掉剪贴板里的链接（等待块里有复制按钮）
      const browserFlow = !response.user_code;
      if (!browserFlow && !linkOnly) {
        try {
          await copyText(response.user_code);
        } catch (e) {
          console.debug("[ManagedAuth] Failed to copy user code:", e);
        }
      }
      if (request.generation !== flowGenerationRef.current) return;
      // Add a small buffer on top of the provider's suggested interval to avoid
      // hitting slow_down responses too aggressively during device polling.
      // The browser flow only checks a local result, so it can poll quickly.
      const interval = browserFlow
        ? 1500
        : Math.max((response.interval || 5) + 3, 8) * 1000;
      let pollInFlight = false;
      const expiresAt = Date.now() + response.expires_in * 1000;
      const pollOnce = async () => {
        if (request.generation !== flowGenerationRef.current) return;
        if (pollInFlight) return;
        if (Date.now() > expiresAt) {
          stopPolling();
          activeDeviceCodeRef.current = null;
          void cancelBackendFlow(response.device_code);
          flowGenerationRef.current += 1;
          setPollingState("error");
          setError(
            t("codexOauth.loginExpired", {
              defaultValue: "Login expired. Please try again.",
            }),
          );
          return;
        }
        try {
          pollInFlight = true;
          const newAccount = await authApi.authPollForAccount(
            authProvider,
            response.device_code,
          );
          if (request.generation !== flowGenerationRef.current) return;
          if (newAccount) {
            stopPolling();
            activeDeviceCodeRef.current = null;
            flowGenerationRef.current += 1;
            const completionGeneration = flowGenerationRef.current;
            setPollingState("success");
            await refetchStatus();
            await queryClient.invalidateQueries({ queryKey });
            if (completionGeneration !== flowGenerationRef.current) return;
            setPollingState("idle");
            setDeviceCode(null);
          }
        } catch (e) {
          if (request.generation !== flowGenerationRef.current) return;
          const errorMessage = e instanceof Error ? e.message : String(e);
          if (
            !errorMessage.includes("pending") &&
            !errorMessage.includes("slow_down")
          ) {
            stopPolling();
            activeDeviceCodeRef.current = null;
            void cancelBackendFlow(response.device_code);
            flowGenerationRef.current += 1;
            setPollingState("error");
            setError(
              authProvider === "codex_oauth" &&
                errorMessage === CODEX_OAUTH_DUPLICATE_ACCOUNT_ERROR
                ? t("codexOauth.duplicateAccount", {
                    defaultValue: "该 ChatGPT 账号已添加，请直接使用现有账号。",
                  })
                : errorMessage,
            );
          }
        } finally {
          pollInFlight = false;
        }
      };
      pollingIntervalRef.current = setInterval(pollOnce, interval);
      pollingTimeoutRef.current = setTimeout(() => {
        if (request.generation !== flowGenerationRef.current) return;
        stopPolling();
        activeDeviceCodeRef.current = null;
        void cancelBackendFlow(response.device_code);
        flowGenerationRef.current += 1;
        setPollingState("error");
        setError(
          t("codexOauth.loginExpired", {
            defaultValue: "Login expired. Please try again.",
          }),
        );
      }, response.expires_in * 1000);
      void pollOnce();
    },
    onError: (e, request) => {
      if (request.generation !== flowGenerationRef.current) return;
      setPollingState("error");
      setError(e instanceof Error ? e.message : String(e));
    },
  });
  const logoutMutation = useMutation({
    mutationFn: async (): Promise<AccountMutationResult> => {
      try {
        await authApi.authLogout(authProvider);
        return { kind: "ok" };
      } catch (error) {
        const partial = parseCredentialsClearedError(error);
        if (partial) return { kind: "partial", message: partial };
        throw error;
      }
    },
    onSuccess: async (result) => {
      setPollingState("idle");
      setDeviceCode(null);
      setError(null);
      queryClient.setQueryData(queryKey, {
        provider: authProvider,
        authenticated: false,
        default_account_id: null,
        accounts: [],
      });
      if (result.kind === "partial") {
        toast.warning(result.message);
      } else {
        toast.success(
          t("managedAuth.loggedOut", {
            defaultValue: "已登出全部账号",
          }),
        );
      }
      await queryClient.invalidateQueries({ queryKey });
    },
    onError: async (e) => {
      console.error("[ManagedAuth] Failed to logout:", e);
      const detail = extractErrorMessage(e) || t("common.unknown");
      setError(detail);
      toast.error(
        t("managedAuth.logoutFailed", {
          defaultValue: "登出失败: {{error}}",
          error: detail,
        }),
      );
      await refetchStatus();
    },
    onSettled: async () => {
      if (authProvider === "codex_oauth") {
        await queryClient.invalidateQueries({
          queryKey: ["providers", "codex"],
        });
      }
    },
  });
  const removeAccountMutation = useMutation({
    mutationFn: async (accountId: string): Promise<AccountMutationResult> => {
      try {
        await authApi.authRemoveAccount(authProvider, accountId);
        return { kind: "ok" };
      } catch (error) {
        const partial = parseCredentialsClearedError(error);
        if (partial) return { kind: "partial", message: partial };
        throw error;
      }
    },
    onSuccess: async (result) => {
      setPollingState("idle");
      setDeviceCode(null);
      setError(null);
      if (result.kind === "partial") {
        toast.warning(result.message);
      } else {
        toast.success(
          t("managedAuth.accountRemoved", {
            defaultValue: "账号已移除",
          }),
        );
      }
      await refetchStatus();
      await queryClient.invalidateQueries({ queryKey });
    },
    onError: async (e) => {
      console.error("[ManagedAuth] Failed to remove account:", e);
      const detail = extractErrorMessage(e) || t("common.unknown");
      setError(detail);
      toast.error(
        t("managedAuth.removeFailed", {
          defaultValue: "删除账号失败: {{error}}",
          error: detail,
        }),
      );
      await refetchStatus();
    },
    onSettled: async () => {
      if (authProvider === "codex_oauth") {
        await queryClient.invalidateQueries({
          queryKey: ["providers", "codex"],
        });
      }
    },
  });
  const setDefaultAccountMutation = useMutation({
    mutationFn: (accountId: string) =>
      authApi.authSetDefaultAccount(authProvider, accountId),
    onSuccess: async () => {
      // Antigravity 的「默认」就是 agy 的全局登录：切完提醒已经开着的 agy 还用原来的号
      if (authProvider === "agy_oauth") {
        const running = await authApi.agyAccountIsRunning().catch(() => false);
        if (running) {
          toast.warning(
            t("authCenter.agy.switchedWhileRunning", {
              defaultValue:
                "已切换。检测到 agy 正在运行：已打开的 agy 仍用原账号，还可能把原账号写回去，请退出后重新打开。",
            }),
          );
        } else {
          toast.success(
            t("authCenter.agy.switched", {
              defaultValue: "已切换，新开的 agy 会使用这个账号。",
            }),
          );
        }
      }
      await refetchStatus();
      await queryClient.invalidateQueries({ queryKey });
      // 没绑账号的 Antigravity 官方卡查的是「agy 现在登着的号」，切完重查，别留着上个号的额度
      if (authProvider === "agy_oauth") {
        await queryClient.invalidateQueries({
          queryKey: subscriptionKeys.quota("agy"),
        });
      }
    },
    onError: (e) => {
      console.error("[ManagedAuth] Failed to set default account:", e);
      const detail = extractErrorMessage(e) || t("common.unknown");
      setError(detail);
      toast.error(
        t("managedAuth.setDefaultFailed", {
          defaultValue: "设为默认失败: {{error}}",
          error: detail,
        }),
      );
    },
  });
  const beginLogin = useCallback(
    (targetAccountId?: string) => {
      const previousDeviceCode = activeDeviceCodeRef.current;
      activeDeviceCodeRef.current = null;
      const generation = flowGenerationRef.current + 1;
      flowGenerationRef.current = generation;
      retryTargetAccountIdRef.current = targetAccountId;
      setPollingState("idle");
      setDeviceCode(null);
      setError(null);
      stopPolling();
      void queueBackendCancellation(previousDeviceCode).then(() => {
        if (generation !== flowGenerationRef.current) return;
        // Claude 官方账号在 Windows 上由 Claude Code 自己开浏览器：不打开时要告诉后端
        const method: ManagedLoginMethod | undefined =
          authProvider === "claude_official" && !readAuthOpenBrowser()
            ? "copy_link"
            : loginMethod;
        startLoginMutation.mutate({
          targetAccountId,
          generation,
          loginMethod: method,
        });
      });
    },
    [
      authProvider,
      queueBackendCancellation,
      startLoginMutation,
      stopPolling,
      loginMethod,
    ],
  );
  const startAuth = useCallback(() => beginLogin(), [beginLogin]);
  const reauthAccount = useCallback(
    (accountId: string) => {
      beginLogin(accountId);
    },
    [beginLogin],
  );
  const retryAuth = useCallback(
    () => beginLogin(retryTargetAccountIdRef.current),
    [beginLogin],
  );
  const cancelAuth = useCallback(() => {
    flowGenerationRef.current += 1;
    const previousDeviceCode = activeDeviceCodeRef.current;
    activeDeviceCodeRef.current = null;
    retryTargetAccountIdRef.current = undefined;
    stopPolling();
    setPollingState("idle");
    setDeviceCode(null);
    setError(null);
    void queueBackendCancellation(previousDeviceCode);
  }, [queueBackendCancellation, stopPolling]);
  const logout = useCallback(async () => {
    cancelAuth();
    await flowTransitionRef.current;
    return logoutMutation.mutateAsync();
  }, [cancelAuth, logoutMutation]);
  const removeAccount = useCallback(
    async (accountId: string) => {
      cancelAuth();
      await flowTransitionRef.current;
      return removeAccountMutation.mutateAsync(accountId);
    },
    [cancelAuth, removeAccountMutation],
  );
  const setDefaultAccount = useCallback(
    (accountId: string) => {
      setDefaultAccountMutation.mutate(accountId);
    },
    [setDefaultAccountMutation],
  );
  const accounts = authStatus?.accounts ?? [];
  return {
    authStatus,
    isLoadingStatus,
    // Distinguish "status loaded successfully" from "loading / failed" so
    // callers don't treat a failed query's empty `accounts` as authoritative.
    isStatusSuccess,
    isStatusError,
    accounts,
    hasAnyAccount: accounts.length > 0,
    isAuthenticated: authStatus?.authenticated ?? false,
    defaultAccountId: authStatus?.default_account_id ?? null,
    pollingState,
    deviceCode,
    error,
    isPolling: pollingState === "polling",
    isAddingAccount: startLoginMutation.isPending || pollingState === "polling",
    isRemovingAccount:
      removeAccountMutation.isPending || logoutMutation.isPending,
    isSettingDefaultAccount: setDefaultAccountMutation.isPending,
    startAuth,
    addAccount: startAuth,
    reauthAccount,
    retryAuth,
    cancelAuth,
    logout,
    removeAccount,
    setDefaultAccount,
    refetchStatus,
  };
}
