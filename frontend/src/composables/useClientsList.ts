import { ref, type Ref } from "vue";
import type { ClientListResponse } from "../types/api";

export function useClientsList(): {
  data: Ref<ClientListResponse | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  lastUpdated: Ref<number | null>;
  load: () => Promise<void>;
} {
  const data = ref<ClientListResponse | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const lastUpdated = ref<number | null>(null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const res = await fetch("/api/clients/list", { cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      data.value = await res.json();
      lastUpdated.value = Date.now();
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { data, loading, error, lastUpdated, load };
}
