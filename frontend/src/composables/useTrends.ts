import { useIntervalFn } from "@vueuse/core";
import type { TrendsResponse, ClientsResponse } from "../types/api";
import { useApi } from "./useApi";

export function useTrends(intervalMs = 600000) {
  const { data: trends, error: trendsError, load: loadTrends } = useApi<TrendsResponse>("/api/trends");
  const { data: clients, error: clientsError, load: loadClients } = useApi<ClientsResponse>("/api/clients");

  async function refresh() {
    await Promise.all([loadTrends(), loadClients()]);
  }

  const { pause, resume } = useIntervalFn(refresh, intervalMs, { immediate: false });

  function start() {
    void refresh();
    resume();
  }

  function stop() {
    pause();
  }

  return { trends, clients, trendsError, clientsError, start, stop };
}
