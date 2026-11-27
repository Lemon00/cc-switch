export const proxyKeys = {
  status: ["proxyStatus"] as const,
  takeoverStatus: ["proxyTakeoverStatus"] as const,
  globalConfig: ["globalProxyConfig"] as const,
  clientHost: ["proxyClientHost"] as const,
  appConfig: (appType: string) => ["appProxyConfig", appType] as const,
};
