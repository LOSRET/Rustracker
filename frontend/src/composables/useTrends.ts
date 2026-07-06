import { ref, type Ref } from "vue";
import { useIntervalFn } from "@vueuse/core";
import type { TrendsResponse, ClientsResponse } from "../types/api";

export function useTrends(intervalMs = 600000): {
  trends: Ref<TrendsResponse | null>;
  clients: Ref<ClientsResponse | null>;
  start: () => void;
  stop: () => void;
} {
  const trends = ref<TrendsResponse | null>(null);
  const clients = ref<ClientsResponse | null>(null);

  async function refresh() {
    try {
      const [trendsRes, clientsRes] = await Promise.all([
        fetch("/api/trends", { cache: "no-store" }),
        fetch("/api/clients", { cache: "no-store" }),
      ]);
      if (trendsRes.ok) trends.value = await trendsRes.json();
      if (clientsRes.ok) clients.value = await clientsRes.json();
    } catch {
      // chart refresh failures are non-critical
    }
  }

  const { pause, resume } = useIntervalFn(refresh, intervalMs, { immediate: false });

  function start() {
    void refresh();
    resume();
  }

  function stop() {
    pause();
  }

  start();
  return { trends, clients, start, stop };
}
