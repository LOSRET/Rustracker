import { ref } from "vue";
import { useFetch, useIntervalFn } from "@vueuse/core";
import type { StatsResponse } from "../types/api";

export function useStats(intervalMs = 5000) {
  const lastUpdated = ref<number | null>(null);
  let prev: StatsResponse | null = null;

  const { data: stats, error, execute } = useFetch(
    "/api/stats",
    { cache: "no-store" },
    {
      immediate: false,
      updateDataOnError: true,
      afterFetch: (ctx) => {
        prev = ctx.data;
        lastUpdated.value = Date.now();
        return ctx;
      },
      onFetchError: () => ({ data: prev }),
    },
  ).get().json<StatsResponse>();

  const { pause } = useIntervalFn(execute, intervalMs, { immediateCallback: true });

  return { stats, error, lastUpdated, stop: pause };
}
