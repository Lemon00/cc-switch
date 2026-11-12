import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  createEditProviderStore,
  useCreateEditProviderStore,
  useEditProviderRequest,
} from "@/components/providers/editProviderStore";
import type { Provider } from "@/types";

const provider = (id: string): Provider => ({
  id,
  name: id.toUpperCase(),
  category: "third_party",
  settingsConfig: {},
});

describe("createEditProviderStore", () => {
  it("notifies subscribers on open, replacement and close only", () => {
    const store = createEditProviderStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    expect(store.getSnapshot()).toBeNull();

    store.open(provider("a"));
    expect(store.getSnapshot()?.provider.id).toBe("a");
    expect(listener).toHaveBeenCalledTimes(1);

    store.open(provider("b"));
    expect(store.getSnapshot()?.provider.id).toBe("b");
    expect(listener).toHaveBeenCalledTimes(2);

    store.close();
    expect(store.getSnapshot()).toBeNull();
    expect(listener).toHaveBeenCalledTimes(3);

    store.close();
    expect(listener).toHaveBeenCalledTimes(3);

    unsubscribe();
    store.open(provider("c"));
    expect(listener).toHaveBeenCalledTimes(3);
  });
});

describe("useEditProviderRequest", () => {
  it("keeps one store per component and reflects the current request", () => {
    const { result, rerender } = renderHook(() => {
      const store = useCreateEditProviderStore();
      return { store, request: useEditProviderRequest(store) };
    });

    expect(result.current.request).toBeNull();
    const store = result.current.store;

    act(() => store.open(provider("a")));
    expect(result.current.request?.provider.id).toBe("a");

    rerender();
    expect(result.current.store).toBe(store);

    act(() => store.open(provider("b")));
    expect(result.current.request?.provider.id).toBe("b");

    act(() => store.close());
    expect(result.current.request).toBeNull();
  });
});
