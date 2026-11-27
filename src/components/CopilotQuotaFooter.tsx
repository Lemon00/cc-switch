import { QuotaBars, QuotaLines } from "@/components/quota/QuotaLines";
import { failedLines, tierLine } from "@/components/quota/quotaRules";
import { useQuotaDisplay } from "@/components/quota/useQuotaDisplay";
import { PROVIDER_TYPES } from "@/config/constants";
import { resolveManagedAccountId } from "@/lib/authBinding";
import { useCopilotQuota } from "@/lib/query/copilot";
import type { ProviderMeta } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import React from "react";
import { useTranslation } from "react-i18next";
interface CopilotQuotaFooterProps {
  meta?: ProviderMeta;
  inline?: boolean;
  /** 是否为当前激活的供应商 */
  isCurrent?: boolean;
}
const CopilotQuotaFooter: React.FC<CopilotQuotaFooterProps> = ({
  meta,
  inline = false,
  isCurrent = false,
}) => {
  const { t } = useTranslation();
  const display = useQuotaDisplay();
  const accountId = resolveManagedAccountId(
    meta,
    PROVIDER_TYPES.GITHUB_COPILOT,
  );
  const {
    data: quota,
    isError,
    error,
    isFetching: loading,
    refetch,
  } = useCopilotQuota(accountId, { enabled: true, autoQuery: isCurrent });
  if (!quota) {
    // IPC reject 且无缓存时仍渲染失败态，避免 footer 消失无法重试
    if (!isError) return null;
    return inline ? (
      <QuotaLines
        lines={failedLines(
          t,
          extractErrorMessage(error) || t("subscription.queryFailed"),
        )}
        queriedAt={null}
        loading={loading}
        onRefresh={refetch}
      />
    ) : null;
  }
  if (!quota.success) {
    return inline ? (
      <QuotaLines
        lines={failedLines(t, quota.error || t("subscription.queryFailed"))}
        queriedAt={quota.queriedAt}
        loading={loading}
        onRefresh={refetch}
      />
    ) : null;
  }
  if (quota.tiers.length === 0) return null;
  const label = t("subscription.copilotPremium", { defaultValue: "Premium" });
  const rows = quota.tiers.map((tier) => ({
    label,
    line: {
      ...tierLine(t, tier, label, { display }),
      detail: quota.plan || undefined,
    },
  }));
  if (inline) {
    return (
      <QuotaLines
        lines={rows.map((row) => row.line)}
        queriedAt={quota.queriedAt}
        loading={loading}
        onRefresh={refetch}
      />
    );
  }
  return (
    <QuotaBars
      className="mt-3"
      title={quota.plan || t("subscription.title")}
      rows={rows}
      queriedAt={quota.queriedAt}
      loading={loading}
      onRefresh={refetch}
    />
  );
};
export default CopilotQuotaFooter;
