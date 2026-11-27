import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ProviderIconBox } from "@/components/ProviderIconBox";
import { QuotaRefreshCell } from "@/components/settings/auth/AccountQuota";
import {
  ManagedAccountsGroup,
  type GroupAccountRow,
} from "@/components/settings/auth/ManagedAccountsGroup";
import {
  dshApi,
  type DshAccountStatus,
  type DshLoginStart,
  type DshWallet,
} from "@/lib/api/dsh";
import { deliverLoginLink } from "@/lib/authLoginLink";
import { toast } from "@/lib/toast";
import { extractErrorMessage } from "@/utils/errorUtils";
import { resolveProviderIcon } from "@/utils/providerIcon";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CardChipBadge } from "./ProviderCard";
import { ProviderCardActions } from "./ProviderCardActions";
import type { CardChip } from "./presentation";
/** 授权中心和两个 DeepSeek Harness 供应商页共用一份查询：登录本来就是 Web / Desktop 共用的 */
export const DSH_ACCOUNT_STATUS_KEY = ["dsh-account-status"] as const;
export function useDshAccountStatus(enabled = true) {
  return useQuery({
    queryKey: DSH_ACCOUNT_STATUS_KEY,
    queryFn: dshApi.accountStatus,
    staleTime: 60000,
    enabled,
  });
}
const CURRENCY_SYMBOL: Record<string, string> = { CNY: "¥", USD: "$" };
function formatWallet({ currency, balance }: DshWallet): string {
  const amount = Number(balance);
  const text = Number.isFinite(amount) ? amount.toFixed(2) : balance;
  const symbol = CURRENCY_SYMBOL[currency];
  return symbol ? `${symbol}${text}` : `${text} ${currency}`;
}
export function accountName(
  status: DshAccountStatus,
  fallback: string,
): string {
  return status.profile?.name ?? status.profile?.contact ?? fallback;
}
/**
 * 右侧额度列：余额一行、赠金一行（DeepSeek Harness 也分开显示，免得把赠金当成充值），
 * 写在别的服务「剩余 xx%」那一格的位置；刷新用各服务共用的「x 分钟前 ↻」。
 */
function DshBalance({
  login,
  status,
  queriedAt,
  loading,
  onRefresh,
}: {
  login: string;
  status: DshAccountStatus;
  queriedAt: number;
  loading: boolean;
  onRefresh: () => void;
}) {
  const { t } = useTranslation();
  const wallets = status.wallets.map(formatWallet).join(" · ");
  const bonus = status.bonusWallets.map(formatWallet).join(" · ");
  // 右边让出倒计时格（52）和间距（8），和别的账号「剩余 xx%」那一格右对齐
  const cell = "me-[60px] self-end text-caption tabular-nums";
  return (
    <>
      <div className="auth-quota-col flex shrink-0 flex-col justify-center gap-0.5">
        {status.error ? (
          <span className={`${cell} text-fg-3`}>
            {t("dsh.account.balanceFailed")}
          </span>
        ) : (
          <>
            <span className={`${cell} text-fg-1`}>
              {t("dsh.account.balance", { amount: wallets || "—" })}
            </span>
            {bonus && (
              <span className={`${cell} text-fg-2`}>
                {t("dsh.account.bonus", { amount: bonus })}
              </span>
            )}
          </>
        )}
      </div>
      <QuotaRefreshCell
        login={login}
        queriedAt={queriedAt}
        loading={loading}
        onRefresh={onRefresh}
      />
    </>
  );
}
const POLL_INTERVAL_MS = 2000;
/**
 * 浏览器登录：后端起本机回调、向平台登记，这里打开授权页并轮询，直到登录写进共用的凭据文件。
 */
export function useDshLogin(onSignedIn: () => void) {
  const { t } = useTranslation();
  const [flow, setFlow] = useState<DshLoginStart | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const flowRef = useRef<DshLoginStart | null>(null);
  const generation = useRef(0);
  const start = useCallback(async () => {
    const currentGeneration = ++generation.current;
    const previous = flowRef.current;
    flowRef.current = null;
    setFlow(null);
    setError(null);
    setStarting(true);
    try {
      if (previous) await dshApi.loginCancel(previous.flowId);
      if (generation.current !== currentGeneration) return;
      const next = await dshApi.loginStart();
      if (generation.current !== currentGeneration) {
        void dshApi.loginCancel(next.flowId).catch(() => {});
        return;
      }
      flowRef.current = next;
      setFlow(next);
      void deliverLoginLink(next.authUrl, t);
    } catch (e) {
      if (generation.current !== currentGeneration) return;
      setFlow(null);
      setError(extractErrorMessage(e) || t("dsh.account.signInFailed"));
    } finally {
      if (generation.current === currentGeneration) setStarting(false);
    }
  }, [t]);
  const cancel = useCallback(() => {
    generation.current += 1;
    const current = flowRef.current;
    flowRef.current = null;
    if (current) void dshApi.loginCancel(current.flowId).catch(() => {});
    setFlow(null);
    setStarting(false);
    setError(null);
  }, []);
  useEffect(() => {
    if (!flow || error) return;
    let polling = false;
    const timer = window.setInterval(() => {
      if (polling || flowRef.current?.flowId !== flow.flowId) return;
      polling = true;
      dshApi
        .loginPoll(flow.flowId)
        .then((done) => {
          if (!done || flowRef.current?.flowId !== flow.flowId) return;
          flowRef.current = null;
          setFlow(null);
          toast.success(t("dsh.account.signedIn"));
          onSignedIn();
        })
        .catch((e) => {
          if (flowRef.current?.flowId !== flow.flowId) return;
          setError(extractErrorMessage(e) || t("dsh.account.signInFailed"));
        })
        .finally(() => {
          polling = false;
        });
    }, POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [flow, error, onSignedIn, t]);
  // 离开页面时撤下还在等的登录，本机回调端口随之关闭
  useEffect(
    () => () => {
      generation.current += 1;
      const current = flowRef.current;
      flowRef.current = null;
      if (current) void dshApi.loginCancel(current.flowId).catch(() => {});
    },
    [],
  );
  return {
    start,
    login: {
      starting,
      polling: Boolean(flow) && !error,
      pollingState: error
        ? ("error" as const)
        : flow
          ? ("polling" as const)
          : ("idle" as const),
      // 没有验证码：等待块只显示「在浏览器里继续」和打开授权页的按钮
      deviceCode: flow
        ? { user_code: "", verification_uri: flow.authUrl }
        : null,
      error,
      onCancel: cancel,
      onRetry: () => void start(),
    },
  };
}
interface DshAccountLoginProps {
  helpSide?: "top" | "bottom";
}
/**
 * 授权中心的 DeepSeek 账号组。登录和 DeepSeek Harness Web 走同一套浏览器授权，结果写进
 * `$DSH_HOME/.credentials.yaml`，Web 与 Desktop 共用（运行中的 DeepSeek Harness 会自动重新读取）；
 * 这里显示账号和余额，可以退出（两端同时退出）。
 */
export function DshAccountLogin({ helpSide }: DshAccountLoginProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const status = useDshAccountStatus();
  const [confirmSignOut, setConfirmSignOut] = useState(false);
  const refreshStatus = useCallback(
    () =>
      void queryClient.invalidateQueries({ queryKey: DSH_ACCOUNT_STATUS_KEY }),
    [queryClient],
  );
  const { start, login } = useDshLogin(refreshStatus);
  const signOut = useMutation({
    mutationFn: dshApi.accountSignOut,
    onSuccess: async () => {
      setConfirmSignOut(false);
      await queryClient.invalidateQueries({ queryKey: DSH_ACCOUNT_STATUS_KEY });
    },
    onError: (error) =>
      toast.error(t("dsh.account.signOutFailed"), {
        description: extractErrorMessage(error) || undefined,
      }),
  });
  const data = status.data;
  const name = data ? accountName(data, t("dsh.account.unnamed")) : "";
  const rows: GroupAccountRow[] =
    data?.signedIn === true
      ? [
          {
            id: "default",
            login: name,
            avatarUrl: data.profile?.avatarUrl,
            details: data.expired
              ? [t("dsh.account.expired")]
              : [
                  data.profile?.name ? data.profile.contact : null,
                  t("dsh.account.shared"),
                ].filter(Boolean),
            isDefault: false,
            needsReauth: data.expired,
            users: [],
            quota: data.expired ? undefined : (
              <DshBalance
                login={name}
                status={data}
                queriedAt={status.dataUpdatedAt}
                loading={status.isFetching}
                onRefresh={() => void status.refetch()}
              />
            ),
            removeLabel: t("dsh.account.signOut"),
          },
        ]
      : [];
  return (
    <ManagedAccountsGroup
      slug="dsh"
      name="DeepSeek Harness"
      iconName="deepseek"
      help={t("dsh.account.help")}
      helpSide={helpSide}
      accounts={rows}
      status={status.isError ? "error" : status.isSuccess ? "ready" : "loading"}
      statusErrorText={t("dsh.account.statusFailed")}
      onRetryStatus={() => void status.refetch()}
      emptyText={t("dsh.account.empty")}
      onAdd={() => void start()}
      canReauth
      reauthOnlyWhenExpired
      onReauth={() => void start()}
      settingDefault={false}
      onRemove={() => setConfirmSignOut(true)}
      removing={signOut.isPending}
      login={login}
    >
      <ConfirmDialog
        isOpen={confirmSignOut}
        title={t("dsh.account.signOut")}
        message={t("dsh.account.signOutConfirm", { name })}
        pending={signOut.isPending}
        confirmText={t("dsh.account.signOut")}
        onConfirm={() => signOut.mutate()}
        onCancel={() => setConfirmSignOut(false)}
      />
    </ManagedAccountsGroup>
  );
}
/**
 * DeepSeek Harness 供应商页「已添加」里的官方账号卡：登录了才有。账号下的模型由 DeepSeek Harness
 * 自带的账号插件提供，不写进 profile，所以这张卡不是供应商行，也不能移除；登录、退出在授权中心。
 */
export function DshAccountCard({ status }: { status: DshAccountStatus }) {
  const { t } = useTranslation();
  const name = accountName(status, t("dsh.account.unnamed"));
  const chips: CardChip[] = [
    {
      key: "official",
      label: t("providerCard.chip.official"),
      tone: "outline",
    },
  ];
  if (status.expired) {
    chips.push({
      key: "expired",
      label: t("dsh.account.expiredChip"),
      tone: "warning",
      title: t("dsh.account.expired"),
    });
  }
  const balance = status.wallets.map(formatWallet).join(" · ");
  return (
    <div className="group relative rounded-panel border border-border bg-surface transition-[border-color,box-shadow,background-color] duration-150 hover:border-border-strong hover:shadow-v7-sm">
      <div className="flex min-h-[58px] items-center gap-3 py-2.5 pe-3 ps-2">
        <span className="w-4 shrink-0" />
        <ProviderIconBox
          icon={resolveProviderIcon("dsh", "deepseek")}
          name={name}
          className="bg-white text-neutral-900"
          fallbackClassName="bg-transparent text-neutral-600"
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <h3
              className="m-0 min-w-0 truncate text-strong text-fg-1"
              title={name}
            >
              {name}
            </h3>
            {chips.map((chip) => (
              <CardChipBadge key={chip.key} chip={chip} />
            ))}
          </div>
          <div className="mt-0.5 truncate text-caption text-fg-2">
            {t("dsh.account.cardSubtitle")}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          {balance && !status.error && !status.expired && (
            <span className="max-w-[208px] truncate text-caption tabular-nums text-fg-2">
              {t("dsh.account.balance", { amount: balance })}
            </span>
          )}
          <ProviderCardActions
            providerName={name}
            presentation={{
              buttons: [],
              chips,
              editDisabledReason: t("dsh.account.editDisabled"),
              deleteDisabledReason: t("dsh.account.managedHint"),
            }}
            onEdit={() => undefined}
            onDelete={() => undefined}
          />
        </div>
      </div>
    </div>
  );
}
