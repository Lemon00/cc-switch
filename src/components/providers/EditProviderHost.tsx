import {
  useEditProviderRequest,
  type EditProviderStore,
} from "@/components/providers/editProviderStore";
import { useLastValidValue } from "@/hooks/useLastValidValue";
import type { AppId } from "@/lib/api";
import type { ProviderEditorSave } from "@/lib/api/providers";
import type { Provider } from "@/types";
import { lazy, useCallback } from "react";
const EditProviderDialog = lazy(() =>
  import("@/components/providers/EditProviderDialog").then((m) => ({
    default: m.EditProviderDialog,
  })),
);
interface EditProviderHostProps {
  store: EditProviderStore;
  appId: AppId;
  onSubmit: (payload: {
    provider: Provider;
    originalId?: string;
    editorSave?: ProviderEditorSave;
  }) => Promise<void> | void;
  isProxyTakeover: boolean;
  currentProviderId: string;
}
/**
 * 编辑页的宿主：只有它订阅「正在编辑哪个供应商」，打开、关闭编辑页时 App 和列表不重渲染。
 * 第一次打开后一直挂着（关闭时 `open=false`），和原来 App 里的写法一致。
 */
export function EditProviderHost({
  store,
  appId,
  onSubmit,
  isProxyTakeover,
  currentProviderId,
}: EditProviderHostProps) {
  const request = useEditProviderRequest(store);
  // 关闭后保留最后一次的内容，供关闭动画期间显示
  const shown = useLastValidValue(request);
  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open) store.close();
    },
    [store],
  );
  if (!shown) return null;
  return (
    <EditProviderDialog
      open={request !== null}
      provider={shown.provider}
      onOpenChange={handleOpenChange}
      onSubmit={onSubmit}
      appId={appId}
      isProxyTakeover={isProxyTakeover}
      isCurrent={shown.provider.id === currentProviderId}
      modeView={shown.modeView}
    />
  );
}
