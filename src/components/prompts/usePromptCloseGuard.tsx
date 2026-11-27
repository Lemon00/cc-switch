import { ConfirmDialog } from "@/components/ConfirmDialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
/** Local back/Escape/cancel actions need the same draft protection as navigation. */
export function usePromptCloseGuard(
  dirty: boolean,
  busy: boolean,
  onClose: () => void,
) {
  const { t } = useTranslation();
  const [confirmClose, setConfirmClose] = useState(false);
  const requestClose = () => {
    if (busy) return;
    if (dirty) setConfirmClose(true);
    else onClose();
  };
  return {
    requestClose,
    closeDialog: (
      <ConfirmDialog
        isOpen={confirmClose}
        title={t("common.unsavedLeaveTitle")}
        message={t("common.unsavedLeaveMessage")}
        confirmText={t("common.unsavedLeaveConfirm")}
        pending={busy}
        onCancel={() => setConfirmClose(false)}
        onConfirm={() => {
          setConfirmClose(false);
          onClose();
        }}
      />
    ),
  };
}
