import { ref, type Ref } from "vue";
import { useIntervalFn } from "@vueuse/core";
import type { StatsResponse } from "../types/api";

export function useStats(intervalMs = 5000): {
  stats: Ref<StatsResponse | null>;
  error: Ref<string | null>;
  loading: Ref<boolean>;
  lastUpdated: Ref<number | null>;
  refresh: () => Promise<void>;
  stop: () => void;
} {
  const stats = ref<StatsResponse | null>(null);
  const error = ref<string | null>(null);
  const loading = ref(true);
  const lastUpdated = ref<number | null>(null);

  async function refresh() {
    try {
      const res = await fetch("/api/stats", { cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      stats.value = await res.json();
      lastUpdated.value = Date.now();
      error.value = null;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  const { pause } = useIntervalFn(refresh, intervalMs, { immediateCallback: true });

  function stop() {
    pause();
  }

  return { stats, error, loading, lastUpdated, refresh, stop };
}
