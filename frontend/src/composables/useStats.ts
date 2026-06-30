import { ref, type Ref } from "vue";
import type { StatsResponse } from "../types/api";

export function useStats(intervalMs = 2000): {
  stats: Ref<StatsResponse | null>;
  error: Ref<string | null>;
  loading: Ref<boolean>;
  refresh: () => Promise<void>;
  stop: () => void;
} {
  const stats = ref<StatsResponse | null>(null);
  const error = ref<string | null>(null);
  const loading = ref(true);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const res = await fetch("/api/stats", { cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      stats.value = await res.json();
      error.value = null;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function start() {
    void refresh();
    timer = setInterval(refresh, intervalMs);
  }

  function stop() {
    if (timer) clearInterval(timer);
  }

  start();
  return { stats, error, loading, refresh, stop };
}
