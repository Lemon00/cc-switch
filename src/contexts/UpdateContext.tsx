import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useRef,
} from "react";
import type { UpdateInfo } from "../lib/updater";
import { checkForUpdate } from "../lib/updater";
import { extractErrorMessage } from "../utils/errorUtils";

interface UpdateContextValue {
  // 更新状态
  hasUpdate: boolean;
  updateInfo: UpdateInfo | null;
  isChecking: boolean;
  error: string | null;

  // 提示状态
  isDismissed: boolean;
  dismissUpdate: () => void;

  // 操作方法
  checkUpdate: () => Promise<boolean>;
  resetDismiss: () => void;
}

const UpdateContext = createContext<UpdateContextValue | undefined>(undefined);

const DISMISSED_VERSION_KEY = "ccswitch:update:dismissedVersion";
const LEGACY_DISMISSED_KEY = "dismissedUpdateVersion";

function readDismissedVersion(): string | null {
  let version: string | null = null;
  try {
    version = localStorage.getItem(DISMISSED_VERSION_KEY);
    if (!version) {
      version = localStorage.getItem(LEGACY_DISMISSED_KEY);
      if (version) {
        localStorage.setItem(DISMISSED_VERSION_KEY, version);
        localStorage.removeItem(LEGACY_DISMISSED_KEY);
      }
    }
  } catch (error) {
    console.warn("Unable to read update notification preference", error);
  }
  return version;
}

function persistDismissedVersion(version: string | null): void {
  try {
    if (version) {
      localStorage.setItem(DISMISSED_VERSION_KEY, version);
    } else {
      localStorage.removeItem(DISMISSED_VERSION_KEY);
    }
    localStorage.removeItem(LEGACY_DISMISSED_KEY);
  } catch (error) {
    console.warn("Unable to save update notification preference", error);
  }
}

export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const [hasUpdate, setHasUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissedVersion, setDismissedVersion] =
    useState(readDismissedVersion);
  const isDismissed =
    updateInfo !== null && dismissedVersion === updateInfo.availableVersion;

  const isCheckingRef = useRef(false);

  const checkUpdate = useCallback(async () => {
    if (isCheckingRef.current) return false;
    isCheckingRef.current = true;
    setIsChecking(true);
    setError(null);

    try {
      const result = await checkForUpdate({ timeout: 30000 });

      if (result.status === "available") {
        setHasUpdate(true);
        setUpdateInfo(result.info);

        return true; // 有更新
      } else {
        setHasUpdate(false);
        setUpdateInfo(null);
        return false; // 已是最新
      }
    } catch (err) {
      console.error("检查更新失败:", err);
      setError(extractErrorMessage(err) || "检查更新失败");
      setHasUpdate(false);
      throw err; // 抛出错误让调用方处理
    } finally {
      setIsChecking(false);
      isCheckingRef.current = false;
    }
  }, []);

  const dismissUpdate = useCallback(() => {
    if (updateInfo?.availableVersion) {
      setDismissedVersion(updateInfo.availableVersion);
      persistDismissedVersion(updateInfo.availableVersion);
    }
  }, [updateInfo?.availableVersion]);

  const resetDismiss = useCallback(() => {
    setDismissedVersion(null);
    persistDismissedVersion(null);
  }, []);

  // 应用启动时自动检查更新
  useEffect(() => {
    // 延迟1秒后检查，避免影响启动体验
    const timer = setTimeout(() => {
      checkUpdate().catch(console.error);
    }, 1000);

    return () => clearTimeout(timer);
  }, [checkUpdate]);

  const value: UpdateContextValue = {
    hasUpdate,
    updateInfo,
    isChecking,
    error,
    isDismissed,
    dismissUpdate,
    checkUpdate,
    resetDismiss,
  };

  return (
    <UpdateContext.Provider value={value}>{children}</UpdateContext.Provider>
  );
}

export function useUpdate() {
  const context = useContext(UpdateContext);
  if (!context) {
    throw new Error("useUpdate must be used within UpdateProvider");
  }
  return context;
}
