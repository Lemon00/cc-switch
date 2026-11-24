import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "@/lib/toast";
import { Button } from "@/components/ui/button";
import MarkdownEditor from "@/components/MarkdownEditor";
import { FullScreenPanel } from "@/components/common/FullScreenPanel";
import { workspaceApi } from "@/lib/api/workspace";
import { useUnsavedChangesState } from "@/lib/unsavedChanges";
import { ConfirmDialog } from "@/components/ConfirmDialog";

interface WorkspaceFileEditorProps {
  filename: string;
  isOpen: boolean;
  onClose: () => void;
}

const WorkspaceFileEditor: React.FC<WorkspaceFileEditorProps> = ({
  filename,
  isOpen,
  onClose,
}) => {
  const { t } = useTranslation();
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState<string | null>(null);
  const [confirmClose, setConfirmClose] = useState(false);
  const [loadedFile, setLoadedFile] = useState<string | null>(null);
  const fileGeneration = useRef(0);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [saving, setSaving] = useState(false);
  const [isDarkMode, setIsDarkMode] = useState(false);
  const dirty =
    isOpen && loadedFile === filename && content !== (savedContent ?? "");
  useUnsavedChangesState(dirty);
  const requestClose = () => {
    if (dirty || saving) setConfirmClose(true);
    else onClose();
  };

  useEffect(() => {
    setIsDarkMode(document.documentElement.classList.contains("dark"));
    const observer = new MutationObserver(() => {
      setIsDarkMode(document.documentElement.classList.contains("dark"));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const generation = ++fileGeneration.current;
    setLoadedFile(null);
    setSaving(false);
    setConfirmClose(false);
    if (!isOpen || !filename) return;

    let cancelled = false;
    workspaceApi
      .readFile(filename)
      .then((data) => {
        if (cancelled) return;
        setContent(data ?? "");
        setSavedContent(data);
        setLoadedFile(filename);
      })
      .catch((err) => {
        if (cancelled) return;
        console.error("Failed to read workspace file:", err);
        toast.error(t("workspace.loadFailed"));
        onCloseRef.current();
      });
    return () => {
      cancelled = true;
      if (fileGeneration.current === generation) fileGeneration.current += 1;
    };
  }, [isOpen, filename, t]);

  const handleSave = useCallback(async () => {
    if (saving || loadedFile !== filename) return;
    const generation = fileGeneration.current;
    setSaving(true);
    try {
      await workspaceApi.writeFile(filename, content, savedContent);
      if (generation !== fileGeneration.current) return;
      setSavedContent(content);
      toast.success(t("workspace.saveSuccess"));
    } catch (err) {
      if (generation !== fileGeneration.current) return;
      console.error("Failed to save workspace file:", err);
      toast.error(`${t("workspace.saveFailed")}: ${String(err)}`);
    } finally {
      if (generation === fileGeneration.current) setSaving(false);
    }
  }, [filename, content, savedContent, saving, loadedFile, t]);

  return (
    <>
      <FullScreenPanel
        // Keep the file list visible until the editor content is ready.
        isOpen={isOpen && loadedFile === filename}
        title={t("workspace.editing", { filename })}
        onClose={requestClose}
        footer={
          <Button onClick={handleSave} disabled={saving}>
            {saving ? t("common.saving") : t("common.save")}
          </Button>
        }
      >
        <MarkdownEditor
          value={content}
          onChange={setContent}
          darkMode={isDarkMode}
          placeholder={`# ${filename}\n\n...`}
          minHeight="calc(100vh - 240px)"
        />
      </FullScreenPanel>
      <ConfirmDialog
        isOpen={confirmClose}
        title={t("common.unsavedLeaveTitle")}
        message={t("common.unsavedLeaveMessage")}
        confirmText={t("common.unsavedLeaveConfirm")}
        pending={saving}
        onCancel={() => setConfirmClose(false)}
        onConfirm={() => {
          setConfirmClose(false);
          onClose();
        }}
      />
    </>
  );
};

export default WorkspaceFileEditor;
