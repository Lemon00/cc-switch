import { QueryClient } from "@tanstack/react-query";
export function createQueryClient({ refetchOnWindowFocus = true } = {}) {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // IPC is local. navigator.onLine cannot tell whether the backend can
        // read files or reach an upstream through its configured proxy.
        networkMode: "always",
        retry: 1,
        refetchOnWindowFocus,
        staleTime: 0,
      },
      mutations: {
        networkMode: "always",
        retry: false,
      },
    },
  });
}
export const queryClient = createQueryClient();
