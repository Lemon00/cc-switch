import type { ProfileScope } from "@/lib/api/profiles";
import { invalidateProfileCaches } from "@/lib/query/profiles";
import { useQueryClient } from "@tanstack/react-query";
import { useTauriEvent } from "./useTauriEvent";
export function useProfileCacheBridge() {
  const queryClient = useQueryClient();
  useTauriEvent<{
    scope?: ProfileScope;
  } | null>("profile-applied", (payload) =>
    invalidateProfileCaches(queryClient, payload?.scope),
  );
}
