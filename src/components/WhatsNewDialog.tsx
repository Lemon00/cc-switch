import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { settingsApi } from "@/lib/api";
import {
  changelogUrl,
  resolveWhatsNewLanguage,
  type WhatsNewEntry,
} from "@/lib/whatsNew";
import { ChevronRight, ExternalLink, Sparkles } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
/** 默认展开的版本数，更早的折叠起来，跳了很多版本也不会撑满弹窗 */
const EXPANDED_VERSIONS = 3;
interface WhatsNewDialogProps {
  open: boolean;
  onClose: () => void;
  /** 新到旧 */
  entries: WhatsNewEntry[];
  /** 启动弹出时传当前版本，标题写「已更新到 vX」；不传是关于页的「近期更新」 */
  updatedTo?: string;
  /** 上次看过的版本，副标题写「自 vX 以来的更新」 */
  since?: string;
}
export function WhatsNewDialog({
  open,
  onClose,
  entries,
  updatedTo,
  since,
}: WhatsNewDialogProps) {
  const { t, i18n } = useTranslation();
  const language = resolveWhatsNewLanguage(i18n.language);
  const [showAll, setShowAll] = useState(false);
  const visible = showAll ? entries : entries.slice(0, EXPANDED_VERSIONS);
  const hiddenCount = entries.length - visible.length;
  const openExternal = (url: string) => {
    settingsApi.openExternal(url).catch((error) => {
      console.error("Failed to open changelog:", error);
    });
  };
  const handleViewFull = () => {
    // 只有一个版本就直接打开这一版，多个版本打开更新日志列表
    openExternal(
      changelogUrl(
        language,
        entries.length === 1 ? entries[0].version : undefined,
      ),
    );
    onClose();
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent
        className="max-w-md"
        zIndex="top"
        {...(since ? {} : { "aria-describedby": undefined })}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-fg-1" />
            {updatedTo
              ? t("whatsNew.updatedTo", { version: updatedTo })
              : t("whatsNew.recentTitle")}
          </DialogTitle>
          {since && (
            <DialogDescription>
              {t("whatsNew.since", { version: since })}
            </DialogDescription>
          )}
        </DialogHeader>
        {/* flex-auto 而不是 flex-1：DialogContent 是 h-fit，WebKit（macOS / Linux）算高度时把
            flex-basis: 0% 的正文按 0 计，弹窗只剩标题和按钮那么高，日志被截成一行 */}
        <div className="min-h-0 flex-auto space-y-5 overflow-y-auto px-6 py-4">
          {visible.map((entry) => (
            <section key={entry.version}>
              <div className="flex items-center justify-between gap-3">
                <h3 className="text-strong tabular-nums text-fg-1">
                  v{entry.version}
                </h3>
                <button
                  type="button"
                  className="inline-flex items-center gap-1 text-caption text-fg-2 hover:text-fg-1"
                  onClick={() =>
                    openExternal(changelogUrl(language, entry.version))
                  }
                >
                  {t("whatsNew.versionDetails")}
                  <ExternalLink className="h-3 w-3" />
                </button>
              </div>
              <ul className="mt-2 space-y-1.5">
                {entry.items.map((item, index) => (
                  <li
                    key={index}
                    className="flex items-start gap-2 text-body text-fg-1"
                  >
                    <span className="mt-0.5 shrink-0 rounded-control bg-subtle px-1.5 text-badge text-fg-2">
                      {t(`whatsNew.types.${item.type}`)}
                    </span>
                    <span className="min-w-0">{item[language]}</span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {hiddenCount > 0 && (
            <button
              type="button"
              className="inline-flex items-center gap-1 text-caption text-fg-2 hover:text-fg-1"
              onClick={() => setShowAll(true)}
            >
              <ChevronRight className="h-3.5 w-3.5" />
              {t("whatsNew.showEarlier", { count: hiddenCount })}
            </button>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={handleViewFull}>
            {t("whatsNew.viewFull")}
          </Button>
          <Button onClick={onClose}>{t("whatsNew.confirm")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
/**
 * 启动时的更新摘要：当前版本比 settings.json 里记的 independentWhatsNewSeenVersion 新，
 * 就把这之间有内容的版本弹给用户看，关掉后记下当前版本。
 * 排在欢迎弹窗和「界面改版了」后面，那两个确认时会顺手记下当前版本，不会接着弹。
 */
