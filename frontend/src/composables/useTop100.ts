import { ref } from "vue";
import type { Top100Response, SortKey } from "../types/api";
import { useApi } from "./useApi";

export function useTop100() {
  const sort = ref<SortKey>("peers");
  const { data, loading, error, lastUpdated, load } = useApi<Top100Response>("/api/top100?limit=100");
  return { data, loading, error, sort, lastUpdated, load };
}
