import { ref, type Ref } from "vue";
import type { TrendsResponse, ClientsResponse } from "../types/api";

export function useTrends(intervalMs = 30000): {
  trends: Ref<TrendsResponse | null>;
  clients: Ref<ClientsResponse | null>;
  refresh: () => Promise<void>;
  stop: () => void;
} {
  const trends = ref<TrendsResponse | null>(null);
  const clients = ref<ClientsResponse | null>(null);
  let timer: ReturnType<typeof setInterval> | null = null;

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

  function start() {
    void refresh();
    timer = setInterval(refresh, intervalMs);
  }

  function stop() {
    if (timer) clearInterval(timer);
  }

  start();
  return { trends, clients, refresh, stop };
}
