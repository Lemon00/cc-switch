import type {
  InvalidateQueryFilters,
  QueryClient,
} from "@tanstack/react-query";
/** Discard pre-write reads, including initial requests with no cached data. */
export async function invalidateQueriesAfterChange(
  client: QueryClient,
  filters: InvalidateQueryFilters = {},
) {
  await client.cancelQueries(filters);
  await client.invalidateQueries(filters);
}
