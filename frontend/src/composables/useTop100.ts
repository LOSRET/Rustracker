import { ref, type Ref } from "vue";
import type { Top100Response, SortKey } from "../types/api";

export function useTop100(): {
  data: Ref<Top100Response | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  sort: Ref<SortKey>;
  load: () => Promise<void>;
} {
  const data = ref<Top100Response | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const sort = ref<SortKey>("peers");

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const res = await fetch("/api/top100?limit=100", { cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      data.value = await res.json();
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { data, loading, error, sort, load };
}
