import { useMemo } from "react";
import type { SessionMeta } from "@/types";

interface UseSessionSearchOptions {
  sessions: SessionMeta[];
  providerFilter: string;
}

interface UseSessionSearchResult {
  search: (query: string) => SessionMeta[];
}

export function useSessionSearch({
  sessions,
  providerFilter,
}: UseSessionSearchOptions): UseSessionSearchResult {
  const filteredByProvider = useMemo(() => {
    if (providerFilter === "all") return sessions;
    return sessions.filter((s) => s.providerId === providerFilter);
  }, [sessions, providerFilter]);

  const search = useMemo(() => {
    let contents: string[] | undefined;
    const normalize = (value: string) =>
      value.normalize("NFKD").toLowerCase().replace(/\p{M}/gu, "");
    return (query: string): SessionMeta[] => {
      const needle = normalize(query.trim());

      if (!needle) {
        return [...filteredByProvider].sort((a, b) => {
          const aTs = a.lastActiveAt ?? a.createdAt ?? 0;
          const bTs = b.lastActiveAt ?? b.createdAt ?? 0;
          return bTs - aTs;
        });
      }

      const terms = needle.match(/[\p{L}\p{N}]+/gu) ?? [];
      if (terms.length === 0) return [];
      contents ??= filteredByProvider.map((session) =>
        normalize(
          [
            session.sessionId,
            session.title,
            session.summary,
            session.projectDir,
            session.sourcePath,
          ]
            .filter(Boolean)
            .join(" "),
        ),
      );
      return filteredByProvider.filter((_, index) =>
        terms.every((term) => contents![index].includes(term)),
      );
    };
  }, [filteredByProvider]);

  return { search };
}
