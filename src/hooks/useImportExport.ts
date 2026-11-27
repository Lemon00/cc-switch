import { settingsApi } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
export type ImportStatus =
  | "idle"
  | "importing"
  | "success"
  | "partial-success"
  | "error";
export interface UseImportExportOptions {
  onImportSuccess?: () => void | Promise<void>;
}
export interface UseImportExportResult {
  selectedFile: string;
  status: ImportStatus;
  errorMessage: string | null;
  backupId: string | null;
  isImporting: boolean;
  selectImportFile: () => Promise<void>;
  clearSelection: () => void;
  importConfig: () => Promise<void>;
  exportConfig: () => Promise<void>;
  resetStatus: () => void;
}
export function useImportExport(
  options: UseImportExportOptions = {},
): UseImportExportResult {
  const { t } = useTranslation();
  const { onImportSuccess } = options;
  const [selectedFile, setSelectedFile] = useState("");
  const [status, setStatus] = useState<ImportStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [backupId, setBackupId] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const importingRef = useRef(false);
  const clearSelection = useCallback(() => {
    if (importingRef.current) return;
    setSelectedFile("");
    setStatus("idle");
    setErrorMessage(null);
    setBackupId(null);
  }, []);
  const selectImportFile = useCallback(async () => {
    if (importingRef.current) return;
    try {
      const filePath = await settingsApi.openFileDialog();
      if (filePath && !importingRef.current) {
        setSelectedFile(filePath);
        setStatus("idle");
        setErrorMessage(null);
        setBackupId(null);
      }
    } catch (error) {
      console.error("[useImportExport] Failed to open file dialog", error);
      toast.error(
        t("settings.selectFileFailed", {
          defaultValue: "选择文件失败",
        }),
      );
    }
  }, [t]);
  const importConfig = useCallback(async () => {
    if (!selectedFile) {
      toast.error(
        t("settings.selectFileFailed", {
          defaultValue: "请选择有效的 SQL 备份文件",
        }),
      );
      return;
    }
    if (importingRef.current) return;
    importingRef.current = true;
    setIsImporting(true);
    setStatus("importing");
    setErrorMessage(null);
    setBackupId(null);
    try {
      const result = await settingsApi.importConfigFromFile(selectedFile);
      if (!result.success) {
        setStatus("error");
        const message =
          result.message ||
          t("settings.configCorrupted", {
            defaultValue: "SQL 文件已损坏或格式不正确",
          });
        setErrorMessage(message);
        toast.error(message);
        return;
      }
      setBackupId(result.backupId ?? null);
      // The backend already projects the restored data while holding the sync
      // lock. Refresh UI only; another projection here can race a newer restore.
      let warning = result.warning;
      try {
        await onImportSuccess?.();
      } catch (error) {
        console.error(
          "[useImportExport] Failed to refresh imported data",
          error,
        );
        const refreshWarning = t("settings.importRefreshFailed", {
          defaultValue: "配置已导入，但界面刷新失败，请重新打开页面。",
        });
        warning = [warning, refreshWarning].filter(Boolean).join("\n");
      }
      if (warning) {
        setStatus("partial-success");
        toast.warning(warning);
      } else {
        setStatus("success");
        toast.success(
          t("settings.importSuccess", {
            defaultValue: "配置导入成功",
          }),
          { closeButton: true },
        );
      }
    } catch (error) {
      console.error("[useImportExport] Failed to import config", error);
      setStatus("error");
      const message =
        error instanceof Error ? error.message : String(error ?? "");
      setErrorMessage(message);
      toast.error(
        t("settings.importFailedError", {
          defaultValue: "导入配置失败: {{message}}",
          message,
        }),
      );
    } finally {
      importingRef.current = false;
      setIsImporting(false);
    }
  }, [onImportSuccess, selectedFile, t]);
  const exportConfig = useCallback(async () => {
    try {
      const now = new Date();
      const stamp = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}_${String(now.getHours()).padStart(2, "0")}${String(now.getMinutes()).padStart(2, "0")}${String(now.getSeconds()).padStart(2, "0")}`;
      const defaultName = `cc-switch-export-${stamp}.sql`;
      const destination = await settingsApi.saveFileDialog(defaultName);
      if (!destination) {
        return;
      }
      const result = await settingsApi.exportConfigToFile(destination);
      if (result.success) {
        const displayPath = result.filePath ?? destination;
        toast.success(
          t("settings.configExported", {
            defaultValue: "配置已导出",
          }) + `\n${displayPath}`,
          { closeButton: true },
        );
      } else {
        toast.error(
          t("settings.exportFailed", {
            defaultValue: "导出配置失败",
          }) + (result.message ? `: ${result.message}` : ""),
        );
      }
    } catch (error) {
      console.error("[useImportExport] Failed to export config", error);
      toast.error(
        t("settings.exportFailedError", {
          defaultValue: "导出配置失败: {{message}}",
          message: error instanceof Error ? error.message : String(error ?? ""),
        }),
      );
    }
  }, [t]);
  const resetStatus = useCallback(() => {
    if (importingRef.current) return;
    setStatus("idle");
    setErrorMessage(null);
    setBackupId(null);
  }, []);
  return {
    selectedFile,
    status,
    errorMessage,
    backupId,
    isImporting,
    selectImportFile,
    clearSelection,
    importConfig,
    exportConfig,
    resetStatus,
  };
}
